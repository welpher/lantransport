; Upgrading runs the previous uninstaller first, and that fails (or stops to
; ask) while the app is still running. Close it up front; taskkill failing just
; means it was not running.
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /IM lantransport.exe /F'
  nsExec::Exec 'taskkill /IM LanTransport.exe /F'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /IM lantransport.exe /F'
  nsExec::Exec 'taskkill /IM LanTransport.exe /F'
!macroend
