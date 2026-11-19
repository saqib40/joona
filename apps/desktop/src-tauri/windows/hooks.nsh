; Joona keeps its settings, history and models in folders named Joona, not
; after the bundle identifier, so "Delete the application data" has to remove
; them here. API keys live in Windows Credential Manager, which only Joona
; itself can clear. Nothing is deleted while updating.

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --forget-keys'
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$APPDATA\Joona"
    RmDir /r "$LOCALAPPDATA\Joona"
  ${EndIf}
!macroend
