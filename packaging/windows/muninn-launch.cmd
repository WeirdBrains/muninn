@echo off
rem Start Menu launcher: opens Muninn in Windows Terminal (or a console window when
rem Windows Terminal is missing). The first run sets up the keys, then the TUI starts.
if "%~1"=="run" goto run
where wt >nul 2>nul
if %errorlevel%==0 (
  start "" wt.exe --title Muninn cmd /c ""%~f0" run"
) else (
  start "Muninn" cmd /c ""%~f0" run"
)
exit /b
:run
rem First run (no keys yet): set them up. Stop here, window open, if that fails, so the
rem TUI never starts without keys; pause after success so the words can be written down.
"%~dp0mnnn.exe" keys status >nul 2>nul
if not errorlevel 1 goto tui
echo First run: Muninn sets up its keys and asks for a new password (typing is hidden).
echo Afterwards it prints 24 recovery words: write them down and keep them offline.
echo.
"%~dp0mnnn.exe" keys init
if errorlevel 1 goto initfailed
echo.
echo Write down the recovery words above now; they are not shown again.
pause
cls
:tui
"%~dp0mnnn.exe" tui
if errorlevel 1 pause
exit /b
:initfailed
echo.
echo Muninn could not set up its keys (see the error above). Nothing was changed;
echo open Muninn again to retry, or run "mnnn keys init" in a terminal.
pause
exit /b 1
