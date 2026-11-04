; Remove the autostart entry that tauri-plugin-autostart writes (value name = product name).
; User data in %APPDATA%\miicrop-pov is left untouched.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Path of Vision"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Path of Vision"
!macroend
