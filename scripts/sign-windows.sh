#!/bin/bash
# Sign the Windows display and the installer scripts with Azure Trusted
# Signing, from Linux, through jsign: glass.exe gets an Authenticode
# signature and a timestamp, the two PowerShell scripts a signature block.
# Windows then knows who published them: SmartScreen stops asking, and
# Smart App Control lets them run.
#
# Needs an Azure login (az) with the Trusted Signing Certificate Profile
# Signer role on the account, and three values:
#   TRUSTED_SIGNING_ENDPOINT   https://weu.codesigning.azure.net (the account's region)
#   TRUSTED_SIGNING_ACCOUNT    the Trusted Signing account
#   TRUSTED_SIGNING_PROFILE    its certificate profile (Public Trust)
# The release workflow runs this when the repository has those values.

set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"
: "${TRUSTED_SIGNING_ENDPOINT:?}" "${TRUSTED_SIGNING_ACCOUNT:?}" "${TRUSTED_SIGNING_PROFILE:?}"

# The two tools are small; the package lists are fetched only when the
# machine's own no longer name them.
apt_install() {
  sudo apt-get install -y --no-install-recommends "$@" \
    || { sudo apt-get update && sudo apt-get install -y --no-install-recommends "$@"; }
}
JSIGN_VERSION=7.5
if ! command -v jsign >/dev/null 2>&1; then
  "$ROOT/scripts/fetch.sh" "https://github.com/ebourg/jsign/releases/download/$JSIGN_VERSION/jsign_${JSIGN_VERSION}_all.deb" /tmp/jsign.deb
  apt_install /tmp/jsign.deb osslsigncode
fi
command -v osslsigncode >/dev/null 2>&1 || apt_install osslsigncode

token=$(az account get-access-token --resource https://codesigning.azure.net --query accessToken -o tsv)
for file in bin/windows-x64/glass.exe remote/windows/install.ps1 remote/windows/uninstall.ps1; do
  echo "sign-windows: $file"
  jsign --storetype TRUSTEDSIGNING \
    --keystore "${TRUSTED_SIGNING_ENDPOINT#https://}" \
    --storepass "$token" \
    --alias "$TRUSTED_SIGNING_ACCOUNT/$TRUSTED_SIGNING_PROFILE" \
    --tsaurl http://timestamp.acs.microsoft.com --tsmode RFC3161 \
    --name "Glass Remote" --url https://github.com/foonerd/glass \
    "$file"
done
# The runner has no Microsoft root certificates, so osslsigncode cannot
# walk the chain and calls the verification failed; Windows walks it.
# What is checked here is what the runner can see: a signature issued by
# Microsoft's public code signing CA, and a timestamp.
echo "sign-windows: verify"
report=$(osslsigncode verify bin/windows-x64/glass.exe 2>&1 || true)
echo "$report" | grep -E "Subject:|verification" | sed 's/^/sign-windows:   /'
echo "$report" | grep -q "Microsoft ID Verified Code Signing PCA" \
  || { echo "sign-windows: glass.exe carries no signature under Microsoft's public code signing CA" >&2; exit 1; }
echo "$report" | grep -q "Timestamping CA" \
  || { echo "sign-windows: glass.exe carries no timestamp" >&2; exit 1; }
echo "sign-windows: signed and timestamped"
