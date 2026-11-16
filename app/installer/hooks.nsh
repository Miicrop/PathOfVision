; Tauri already removes HKCU\...\Run\"Path of Vision" itself (and keeps it on updates).
; It does not touch the StartupApproved entry Windows adds for the autostart value, so
; remove that on a real uninstall only. User data in %APPDATA%\miicrop-pov is left untouched.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Path of Vision"
  ${EndIf}
!macroend

; Shipped files a release no longer contains stay behind when an update installs over an old
; version. The read-only Default profile was dropped with the 3.30 game data.
!macro NSIS_HOOK_POSTINSTALL
  Delete "$INSTDIR\data\profiles\default.json"
!macroend
