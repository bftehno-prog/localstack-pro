!macro NSIS_HOOK_POSTINSTALL
  SetShellVarContext current
  CreateShortCut "$DESKTOP\LocalStack Pro.lnk" "$INSTDIR\localstack-pro.exe" "" "$INSTDIR\localstack-pro.exe" 0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  SetShellVarContext current
  Delete "$DESKTOP\LocalStack Pro.lnk"
!macroend
