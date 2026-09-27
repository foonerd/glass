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

JSIGN_VERSION=7.5
if ! command -v jsign >/dev/null 2>&1; then
  curl -fsSL -o /tmp/jsign.deb "https://github.com/ebourg/jsign/releases/download/$JSIGN_VERSION/jsign_${JSIGN_VERSION}_all.deb"
  sudo apt-get install -y --no-install-recommends /tmp/jsign.deb osslsigncode
fi
command -v osslsigncode >/dev/null 2>&1 || sudo apt-get install -y --no-install-recommends osslsigncode

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
echo "sign-windows: verify"
osslsigncode verify bin/windows-x64/glass.exe | grep -E "Signature verification|Subject:" | sed 's/^/sign-windows:   /'
