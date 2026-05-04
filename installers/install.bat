@echo off
rem Rach installer for Windows (cmd.exe). Delegates to install.ps1 — downloads a
rem pre-built binary from GitHub Releases. No Rust toolchain required.
rem
rem Usage:
rem   installers\install.bat [INSTALL_DIR]
rem
rem Default INSTALL_DIR = %LOCALAPPDATA%\Programs\rach (no Administrator needed).

setlocal EnableExtensions

set "SCRIPT_DIR=%~dp0"
set "PS_SCRIPT=%SCRIPT_DIR%install.ps1"

if not exist "%PS_SCRIPT%" (
    echo [xx] %PS_SCRIPT% not found 1>&2
    exit /b 1
)

if not "%~1"=="" (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%PS_SCRIPT%" -InstallDir "%~1"
) else (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%PS_SCRIPT%"
)

exit /b %ERRORLEVEL%
