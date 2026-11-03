// The lite's PCM frames as a page or a test takes them: one Float32Array
// per channel from a binary frame of the segment's shape, the segment's
// formats as outcast's contract names them (s16le, s24le, s32le, f32le,
// interleaved, little-endian). The same code for the browser (GlassPcm on
// the window, loaded by Anymote and the Face tab) and for node (the tests).
(function (root, factory) {
  if (typeof module === 'object' && module && module.exports) module.exports = factory();
  else root.GlassPcm = factory();
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';

  var BYTES = { s16le: 2, s24le: 3, s32le: 4, f32le: 4 };

  function bytesPerSample(format) { return BYTES[format] || 0; }

  // Whole frames in a binary frame of that many bytes.
  function framesIn(segment, byteLength) {
    var fb = segment && (segment.frame_bytes | 0);
    return fb > 0 ? Math.floor(byteLength / fb) : 0;
  }

  // Whether frames of this segment can be made into sound here: a known
  // format with whole frames; one-bit audio (DSD) cannot.
  function playable(segment) {
    if (!segment || segment.one_bit) return false;
    var channels = segment.channels | 0;
    var bps = bytesPerSample(segment.format);
    return channels >= 1 && bps > 0 && (segment.frame_bytes | 0) >= bps * channels && (segment.rate | 0) > 0;
  }

  // The samples of one binary frame, a Float32Array per channel in -1..1;
  // null where the segment is not playable.
  function decode(segment, buffer) {
    if (!playable(segment)) return null;
    var channels = segment.channels | 0;
    var bps = bytesPerSample(segment.format);
    var fb = segment.frame_bytes | 0;
    var frames = framesIn(segment, buffer.byteLength);
    var out = [];
    for (var c = 0; c < channels; c++) out.push(new Float32Array(frames));
    var view = new DataView(buffer);
    var format = segment.format;
    for (var i = 0; i < frames; i++) {
      var base = i * fb;
      for (var ch = 0; ch < channels; ch++) {
        var p = base + ch * bps;
        var v;
        if (format === 's16le') v = view.getInt16(p, true) / 32768;
        else if (format === 's24le') v = (view.getUint8(p) | (view.getUint8(p + 1) << 8) | (view.getInt8(p + 2) << 16)) / 8388608;
        else if (format === 's32le') v = view.getInt32(p, true) / 2147483648;
        else v = view.getFloat32(p, true);
        out[ch][i] = v;
      }
    }
    return out;
  }

  return { decode: decode, framesIn: framesIn, bytesPerSample: bytesPerSample, playable: playable };
});
