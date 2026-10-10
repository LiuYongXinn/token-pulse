; The updater launches this installer before requesting an ordinary Tauri exit.
; Wait before Tauri's Restart Manager check, so it cannot force-kill the parent
; while the taskbar host restores Explorer geometry and workers finish shutdown.
!macro NSIS_HOOK_PREINSTALL
  Push $0
  Push $1
  Push $2
  ClearErrors
  ${GetOptions} $CMDLINE "/TOKENPULSE_PARENT=" $0
  ${IfNot} ${Errors}
    ; These are generated numeric process IDs, never file names or shell commands.
    IntCmp $0 0 tp_parent_invalid tp_parent_invalid tp_parent_open
    tp_parent_open:
      ; Capture last error inside the same System plug-in call. A later
      ; System::Call GetLastError observes plug-in bookkeeping instead.
      System::Call 'kernel32::OpenProcess(i 0x00100000, i 0, i r0) p.r1 ?e'
      Pop $2
      ${If} $1 != 0
        System::Call 'kernel32::WaitForSingleObject(p r1, i 30000) i.r2'
        System::Call 'kernel32::CloseHandle(p r1)'
        ${If} $2 != 0
          MessageBox MB_OK|MB_ICONEXCLAMATION "TokenPulse has not finished closing. Please close it and run the update again."
          Abort
        ${EndIf}
      ${Else}
        ; ERROR_INVALID_PARAMETER means the process has already exited.
        ${If} $2 != 87
          MessageBox MB_OK|MB_ICONEXCLAMATION "TokenPulse could not confirm that the previous app has closed. Please close it and run the update again."
          Abort
        ${EndIf}
      ${EndIf}
      Goto tp_parent_done
    tp_parent_invalid:
      MessageBox MB_OK|MB_ICONEXCLAMATION "Invalid TokenPulse update parent. Please run the installer again."
      Abort
    tp_parent_done:
  ${EndIf}
  Pop $2
  Pop $1
  Pop $0
!macroend
