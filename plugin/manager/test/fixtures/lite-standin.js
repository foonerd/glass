'use strict';
// A stand-in for outcast-lite, for the tests of playhere.js: it speaks the
// lite's standard output (one JSON line per event) and enough of its wire
// (a WebSocket upgrade answered, `hello`, `segment`, two binary frames of
// 20 ms, `listeners`) to prove the Manager's start, proxy and follow. The
// arguments are the lite's; `--idle N` is honoured, the rest read and
// ignored. LITE_STANDIN=fail leaves at once with code 3 (a lite that cannot
// run); LITE_STANDIN=mute never says its port.
const net = require('net');
const crypto = require('crypto');

const args = process.argv.slice(2);
let idle = 30;
for (let i = 0; i < args.length; i++) if (args[i] === '--idle') idle = parseInt(args[i + 1], 10) || 0;
const say = (o) => process.stdout.write(JSON.stringify(o) + '\n');

if (process.env.LITE_STANDIN === 'fail') {
  process.stderr.write('outcast-lite: no ring for tag glass can ever be read here\n');
  process.exit(3);
}

const SEGMENT = { type: 'segment', rate: 44100, channels: 2, format: 's16le', bits: 16, frame_bytes: 4, one_bit: false, capped: false, chunk_ms: 20 };

function frame(opcode, payload) {
  const len = payload.length;
  let head;
  if (len < 126) head = Buffer.from([0x80 | opcode, len]);
  else { head = Buffer.alloc(4); head[0] = 0x80 | opcode; head[1] = 126; head.writeUInt16BE(len, 2); }
  return Buffer.concat([head, payload]);
}
const text = (o) => frame(1, Buffer.from(JSON.stringify(o)));

// 20 ms of s16le stereo at 44.1 kHz: 882 frames, a ramp on the left and
// its negative on the right, so a reader can tell the channels apart.
function pcm(step) {
  const frames = 882;
  const b = Buffer.alloc(frames * 4);
  for (let i = 0; i < frames; i++) {
    const v = ((i + step * frames) % 2000) - 1000;
    b.writeInt16LE(v, i * 4);
    b.writeInt16LE(-v, i * 4 + 2);
  }
  return b;
}

let listeners = 0;
let lastListener = Date.now();
const server = net.createServer(function (socket) {
  let got = '';
  socket.once('data', function (chunk) {
    got += chunk.toString('latin1');
    const end = got.indexOf('\r\n\r\n');
    if (end === -1) { socket.destroy(); return; }
    const m = /sec-websocket-key:\s*([^\r\n]+)/i.exec(got);
    if (!m) { socket.end('HTTP/1.1 400 Bad Request\r\n\r\n'); return; }
    const accept = crypto.createHash('sha1').update(m[1].trim() + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
    socket.write('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ' + accept + '\r\n\r\n');
    listeners++;
    say({ type: 'listeners', count: listeners });
    socket.write(text({ type: 'hello', version: '0.0.0-standin', tag: 'glass' }));
    say(SEGMENT);
    socket.write(text(SEGMENT));
    socket.write(frame(2, pcm(0)));
    socket.write(frame(2, pcm(1)));
    socket.on('data', function () { /* a ping or a close from the client: nothing to answer here */ });
    socket.on('close', function () { listeners--; lastListener = Date.now(); say({ type: 'listeners', count: listeners }); });
    socket.on('error', function () {});
  });
});
server.listen(0, '127.0.0.1', function () {
  if (process.env.LITE_STANDIN === 'mute') return;
  say({ type: 'listening', address: '127.0.0.1:' + server.address().port, tag: 'glass' });
});
process.on('SIGTERM', function () { say({ type: 'exit', reason: 'signal', seconds: 0 }); process.exit(0); });
if (idle > 0) {
  setInterval(function () {
    if (listeners === 0 && Date.now() - lastListener >= idle * 1000) {
      say({ type: 'exit', reason: 'idle', seconds: idle });
      process.exit(0);
    }
  }, 100).unref();
  // Kept alive by the server; the interval alone would not hold it.
}
