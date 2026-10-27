; Kairo's installer adds a Start menu entry for Kairo itself, which opens the command palette.
; These add one for Chrono, the screen time tracker, which opens its window, and take it away
; again on uninstall.

!macro NSIS_HOOK_POSTINSTALL
  CreateShortCut "$SMPROGRAMS\Chrono.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "--chrono" "$INSTDIR\${MAINBINARYNAME}.exe" 0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$SMPROGRAMS\Chrono.lnk"
!macroend
