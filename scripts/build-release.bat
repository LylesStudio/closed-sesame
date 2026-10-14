@echo off
setlocal
cd /d "%~dp0.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build-release.ps1" %*
set EXITCODE=%ERRORLEVEL%
if not "%EXITCODE%"=="0" (
    echo.
    echo Build failed with exit code %EXITCODE%.
)
rem Pause only when the script was double-clicked, not when run from an existing console.
echo %cmdcmdline% | find /i "%~f0" >nul
if not errorlevel 1 pause
exit /b %EXITCODE%
