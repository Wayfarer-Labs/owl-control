; OWL Control Installer Script
; NSIS Modern User Interface

!define PRODUCT_NAME "OWL Control"
!define PRODUCT_VERSION "${VERSION}"
!define PRODUCT_PUBLISHER "Wayfarer Labs"
!define PRODUCT_WEB_SITE "https://wayfarerlabs.ai/"
!define PRODUCT_DIR_REGKEY "Software\Microsoft\Windows\CurrentVersion\App Paths\OWL Control.exe"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define PRODUCT_UNINST_ROOT_KEY "HKCU"

; MUI Settings
!include "MUI2.nsh"
!include "FileFunc.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "${NSISDIR}\Contrib\Graphics\Header\nsis3-grey.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP "${NSISDIR}\Contrib\Graphics\Wizard\nsis3-grey.bmp"

; Welcome page
!insertmacro MUI_PAGE_WELCOME

; License page
!define MUI_LICENSEPAGE_CHECKBOX
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"

; Directory page
!insertmacro MUI_PAGE_DIRECTORY

; Instfiles page
!insertmacro MUI_PAGE_INSTFILES

; Finish page
!define MUI_FINISHPAGE_RUN "$INSTDIR\OWL Control.exe"
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_INSTFILES

; Language files
!insertmacro MUI_LANGUAGE "English"

; Installer attributes
Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "..\dist\OWL-Control-Setup-${PRODUCT_VERSION}.exe"
InstallDir "$LOCALAPPDATA\OWL Control"
InstallDirRegKey HKCU "${PRODUCT_DIR_REGKEY}" ""
ShowInstDetails show
ShowUnInstDetails show
RequestExecutionLevel user

; Version Information
!if "${PRODUCT_VERSION}" == "dev"
  !define VI_PRODUCT_VERSION "0.0.0.0"
!else
  !define VI_PRODUCT_VERSION "${PRODUCT_VERSION}.0.0"
!endif
VIProductVersion "${VI_PRODUCT_VERSION}"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey "FileVersion" "${PRODUCT_VERSION}"
VIAddVersionKey "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} Installer"
VIAddVersionKey "LegalCopyright" "Copyright © 2025 ${PRODUCT_PUBLISHER}"

; Function to check if previous versions of owl-control exist, and run uninstaller that will maintain data_dump folder
Function .onInit
  ; Check if already installed
  ReadRegStr $0 ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "UninstallString"
  StrCmp $0 "" done

  ; Found existing installation
  MessageBox MB_OKCANCEL|MB_ICONQUESTION \
    "${PRODUCT_NAME} is already installed. $\n$\nClick 'OK' to remove the previous version and continue, or 'Cancel' to cancel this installation." \
    IDOK uninst
  Abort

uninst:
  ; Get the installation directory - try InstallLocation first
  ReadRegStr $INSTDIR ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "InstallLocation"

  ; If InstallLocation doesn't exist (older versions), try other methods
  ${If} $INSTDIR == ""
    ; Try getting from the App Paths registry
    ReadRegStr $INSTDIR HKCU "${PRODUCT_DIR_REGKEY}" ""
    ${If} $INSTDIR != ""
      ; Extract directory from full path (removes the exe filename)
      ${GetParent} $INSTDIR $INSTDIR
    ${EndIf}
  ${EndIf}

  ; If still empty, extract from UninstallString
  ${If} $INSTDIR == ""
    ; UninstallString typically contains the full path to uninst.exe
    ${GetParent} $0 $INSTDIR
  ${EndIf}

  ; Clear errors
  ClearErrors

  ; Run the uninstaller silently
  ExecWait '$0 /S _?=$INSTDIR'

  ; Delete the uninstaller after it finishes
  Delete $0

  ; Clean up registry
  DeleteRegKey ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}"

done:
FunctionEnd

Section "MainSection" SEC01
  SetOutPath "$INSTDIR"
  SetOverwrite ifnewer

  ; Install Visual C++ Redistributable if needed
  ${ifNot} ${FileExists} "$SYSDIR\msvcp140.dll"
    DetailPrint "Installing Visual C++ Redistributable..."
    File /oname=$PLUGINSDIR\vc_redist.x64.exe "downloads\vc_redist.x64.exe"
    ExecWait '"$PLUGINSDIR\vc_redist.x64.exe" /norestart'
  ${endIf}

  ; Copy all files and folders from dist directory
  File /r /x "OWL-Control-Setup-*.exe" "..\dist\*.*"
  File "owl-logo.ico"

  ; Create shortcuts
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\OWL Control.exe" "" "$INSTDIR\owl-logo.ico" 0
  CreateShortcut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\OWL Control.exe" "" "$INSTDIR\owl-logo.ico" 0
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\uninst.exe"
SectionEnd

Section -AdditionalIcons
  WriteIniStr "$INSTDIR\${PRODUCT_NAME}.url" "InternetShortcut" "URL" "${PRODUCT_WEB_SITE}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Website.lnk" "$INSTDIR\${PRODUCT_NAME}.url"
SectionEnd

Section -Post
  WriteUninstaller "$INSTDIR\uninst.exe"

  ; Registry keys
  WriteRegStr HKCU "${PRODUCT_DIR_REGKEY}" "" "$INSTDIR\OWL Control.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayName" "$(^Name)"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "UninstallString" "$INSTDIR\uninst.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\OWL Control.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "InstallLocation" "$INSTDIR"

  ; Get installation size
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "EstimatedSize" "$0"
SectionEnd

; Uninstaller
Function un.onUninstSuccess
  HideWindow
  MessageBox MB_ICONINFORMATION|MB_OK "$(^Name) was successfully removed from your computer. data_dump folder was not removed and can be found in the installation directory if it still exists."
FunctionEnd

Function un.onInit
  MessageBox MB_ICONQUESTION|MB_YESNO|MB_DEFBUTTON2 "Are you sure you want to completely remove $(^Name) and all of its components?" IDYES +2
  Abort
FunctionEnd

Section Uninstall
  ; Remove shortcuts first
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Website.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

  ; Remove all subdirectories except data_dump
  RMDir /r "$INSTDIR\resources"
  RMDir /r "$INSTDIR\assets"
  RMDir /r "$INSTDIR\data"
  RMDir /r "$INSTDIR\iconengines"
  RMDir /r "$INSTDIR\obs-plugins"
  RMDir /r "$INSTDIR\platforms"
  RMDir /r "$INSTDIR\rtmp-services"
  RMDir /r "$INSTDIR\styles"
  RMDir /r "$INSTDIR\text-freetype2"
  RMDir /r "$INSTDIR\win-capture"

  ; Remove all other files in root directory
  Delete "$INSTDIR\*.*"

  ; Try to remove the installation directory
  ; This will only succeed if empty or only contains data_dump
  RMDir "$INSTDIR"

  ; Remove registry keys
  DeleteRegKey ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}"
  DeleteRegKey HKCU "${PRODUCT_DIR_REGKEY}"

  SetAutoClose true
SectionEnd