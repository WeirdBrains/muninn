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
"%~dp0mnnn.exe" keys status >nul 2>nul
if errorlevel 1 (
  echo First run: Muninn sets up its keys. Write down the 24 words it prints and keep them offline.
  echo.
  "%~dp0mnnn.exe" keys init
  if errorlevel 1 (
    pause
    exit /b 1
  )
)
"%~dp0mnnn.exe" tui
if errorlevel 1 pause
