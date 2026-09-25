@echo off
REM Authenticode signing for one artifact, called by Tauri during bundling.
REM
REM Tauri invokes this once per bundled file, with the path as %1, and it must run
REM *during* the bundle rather than after it: the updater artifact is a zip of the
REM installer plus a minisign signature over that zip. Sign the installer after
REM bundling and the zip still holds the unsigned copy, while the signature it
REM ships describes a file nobody will ever download. Wiring this through
REM bundle.windows.signCommand is what keeps those two in step.
REM
REM Configuration comes from the environment so nothing about the signing identity
REM is committed. The credentials themselves are read from the environment by the
REM Azure SDK inside trusted-signing-cli (AZURE_CLIENT_ID, AZURE_CLIENT_SECRET,
REM AZURE_TENANT_ID) and are never referenced here.
REM
REM   RFM_SIGN_ENDPOINT   e.g. https://eus.codesigning.azure.net
REM   RFM_SIGN_ACCOUNT    Trusted Signing account name
REM   RFM_SIGN_PROFILE    certificate profile name
REM
REM This deliberately fails the build when it is reachable but misconfigured. A
REM sign step that quietly does nothing produces an unsigned installer that looks
REM signed in the release notes, which is the one outcome worth crashing over.

setlocal

if "%~1"=="" (
  echo sign.cmd: no file to sign was given 1>&2
  exit /b 1
)

if "%RFM_SIGN_ENDPOINT%"=="" goto :missing
if "%RFM_SIGN_ACCOUNT%"=="" goto :missing
if "%RFM_SIGN_PROFILE%"=="" goto :missing

where trusted-signing-cli >nul 2>nul
if errorlevel 1 (
  echo sign.cmd: trusted-signing-cli is not on PATH 1>&2
  echo sign.cmd: install it with `cargo install trusted-signing-cli` 1>&2
  exit /b 1
)

echo sign.cmd: signing %~nx1
trusted-signing-cli -e "%RFM_SIGN_ENDPOINT%" -a "%RFM_SIGN_ACCOUNT%" -c "%RFM_SIGN_PROFILE%" "%~1"
if errorlevel 1 (
  echo sign.cmd: signing failed for %~nx1 1>&2
  exit /b 1
)
exit /b 0

:missing
echo sign.cmd: RFM_SIGN_ENDPOINT, RFM_SIGN_ACCOUNT and RFM_SIGN_PROFILE must all be set 1>&2
exit /b 1
