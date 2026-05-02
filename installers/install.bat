@echo off
rem Rach installer for Windows (cmd.exe).
rem Usage: installers\install.bat [INSTALL_DIR]
rem Default INSTALL_DIR = %ProgramFiles%\rach
rem Run from an Administrator cmd if writing to Program Files.

setlocal EnableExtensions EnableDelayedExpansion

set "REPO_ROOT=%~dp0.."
pushd "%REPO_ROOT%" >nul

if "%~1"=="" (
    set "INSTALL_DIR=%ProgramFiles%\rach"
) else (
    set "INSTALL_DIR=%~1"
)

where cargo >nul 2>&1
if errorlevel 1 (
    echo [xx] cargo not found in PATH. Install Rust from https://rustup.rs 1>&2
    popd >nul & exit /b 1
)

echo ==^> Building Rach (release)...
cargo build --release
if errorlevel 1 (
    echo [xx] build failed 1>&2
    popd >nul & exit /b 1
)

set "SRC_BIN=%REPO_ROOT%\target\release\rach.exe"
if not exist "%SRC_BIN%" (
    echo [xx] build did not produce %SRC_BIN% 1>&2
    popd >nul & exit /b 1
)

echo ==^> Installing to %INSTALL_DIR%\rach.exe
if not exist "%INSTALL_DIR%" (
    mkdir "%INSTALL_DIR%" 2>nul
    if errorlevel 1 (
        echo [xx] cannot create %INSTALL_DIR% — try running as Administrator 1>&2
        popd >nul & exit /b 1
    )
)
copy /Y "%SRC_BIN%" "%INSTALL_DIR%\rach.exe" >nul
if errorlevel 1 (
    echo [xx] copy failed — try running as Administrator 1>&2
    popd >nul & exit /b 1
)

echo ==^> Verifying...
"%INSTALL_DIR%\rach.exe" version
echo.
echo Installed. Add %INSTALL_DIR% to PATH if it is not already there.
echo Try:  rach examples\hello.rach

popd >nul
endlocal
