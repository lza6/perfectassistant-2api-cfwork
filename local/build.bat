@echo off
REM PerfectAssistant2API 本地网关构建
cd /d "%~dp0"
cargo build --release
if %errorlevel% neq 0 (
  echo 构建失败
  exit /b %errorlevel%
)
echo.
echo 构建完成: target\release\perfectassistant2api.exe
echo 运行:   target\release\perfectassistant2api.exe --config config.json
