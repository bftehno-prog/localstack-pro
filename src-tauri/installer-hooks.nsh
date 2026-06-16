!define MUI_WELCOMEPAGE_TITLE "LocalStack Pro"
!define MUI_WELCOMEPAGE_TEXT "Install the Windows local web-development stack by Farid Leonov.$\r$\n$\r$\nThe installer will add LocalStack Pro, bundled services, desktop shortcut and application data folders for the current user.$\r$\n$\r$\nCreator: Farid Leonov  |  https://artnext.ru"
!define MUI_UNWELCOMEPAGE_TITLE "Uninstall LocalStack Pro"
!define MUI_UNWELCOMEPAGE_TEXT "Remove LocalStack Pro from this Windows account.$\r$\n$\r$\nYour project folders and external site files are not deleted by the uninstaller."
!define MUI_FINISHPAGE_TITLE "LocalStack Pro is ready"
!define MUI_FINISHPAGE_TEXT "LocalStack Pro has been installed with the lime desktop icon and Windows tray integration."
!define MUI_UNFINISHPAGE_TITLE "LocalStack Pro was removed"
!define MUI_UNFINISHPAGE_TEXT "LocalStack Pro application files were removed from this Windows account."

!macro NSIS_HOOK_POSTINSTALL
  SetShellVarContext current
  CreateShortCut "$DESKTOP\LocalStack Pro.lnk" "$INSTDIR\localstack-pro.exe" "" "$INSTDIR\localstack-pro.exe" 0 SW_SHOWNORMAL "" "LocalStack Pro"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  SetShellVarContext current
  Delete "$DESKTOP\LocalStack Pro.lnk"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  SetShellVarContext current
  Delete "$DESKTOP\LocalStack Pro.lnk"
!macroend
