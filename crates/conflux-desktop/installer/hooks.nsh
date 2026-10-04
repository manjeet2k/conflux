; NSIS installer hooks for Conflux (bundle.windows.nsis.installerHooks).
; Tauri's installer.nsi expands these macros at fixed points; macros that are not defined
; here are skipped.
;
; Uninstall policy
;   * The user's downloaded files are NEVER deleted. Conflux does not store downloads under
;     the app data folders, and nothing in this file (or in Tauri's template) removes the
;     download folder.
;   * Settings, download history and logs live in "%APPDATA%\com.conflux.desktop" and
;     "%LOCALAPPDATA%\com.conflux.desktop". Tauri's uninstall page has a "Delete the
;     application data" check box (unchecked by default) that removes exactly those two folders.
;   * If the box was left unchecked, we ask once more (default answer: keep) so the choice is
;     explicit and the dialog says plainly that downloads are not touched. Silent/passive
;     uninstalls and updates never ask and never delete anything.

!macro NSIS_HOOK_POSTINSTALL
  ; Register conflux:// URL protocol handler
  WriteRegStr HKCU "Software\Classes\conflux" "" "URL:Conflux Protocol"
  WriteRegStr HKCU "Software\Classes\conflux" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\conflux\DefaultIcon" "" "$INSTDIR\conflux-desktop.exe,0"
  WriteRegStr HKCU "Software\Classes\conflux\shell\open\command" "" '"$INSTDIR\conflux-desktop.exe" "%1"'

  ; Register Native Messaging Host for Chrome, Edge, Brave, and Firefox
  WriteRegStr HKCU "Software\Google\Chrome\NativeMessagingHosts\com.conflux.desktop" "" "$INSTDIR\com.conflux.desktop.json"
  WriteRegStr HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.conflux.desktop" "" "$INSTDIR\com.conflux.desktop.json"
  WriteRegStr HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.conflux.desktop" "" "$INSTDIR\com.conflux.desktop.json"
  WriteRegStr HKCU "Software\Mozilla\NativeMessagingHosts\com.conflux.desktop" "" "$INSTDIR\com.conflux.desktop.firefox.json"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Clean up conflux:// protocol and native messaging registry entries
  DeleteRegKey HKCU "Software\Classes\conflux"
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.conflux.desktop"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.conflux.desktop"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.conflux.desktop"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\com.conflux.desktop"

  ${If} $DeleteAppDataCheckboxState <> 1
  ${AndIf} $UpdateMode <> 1
  ${AndIf} $PassiveMode <> 1
  ${AndIfNot} ${Silent}
    SetShellVarContext current
    ${If} ${FileExists} "$APPDATA\${BUNDLEID}\*.*"
    ${OrIf} ${FileExists} "$LOCALAPPDATA\${BUNDLEID}\*.*"
      MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 \
        "Also remove Conflux settings, download history and logs?$\r$\n$\r$\nYour downloaded files are never deleted." \
        IDNO conflux_keep_user_data
      RmDir /r "$APPDATA\${BUNDLEID}"
      RmDir /r "$LOCALAPPDATA\${BUNDLEID}"
      conflux_keep_user_data:
    ${EndIf}
  ${EndIf}
!macroend
