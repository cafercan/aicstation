; AI C-Station - Inno Setup installer (per-user, no admin rights).
; Built by scripts/build-installer.ps1, which passes /DAppVersion=<version from tauri.conf.json>.
; The AppId must never change: C Launcher finds the installed version through it.

#ifndef AppVersion
  #error Pass /DAppVersion=x.y.z (scripts/build-installer.ps1 does this)
#endif

#define AppName "AI C-Station"
#define AppFileName "AICStation"
#define AppPublisher "cafercan"
#define AppExeName "aicstation.exe"
#define AppUrl "https://github.com/cafercan/aicstation"
#define AppUserModelId "com.cafercan.sessionstation"

[Setup]
AppId={{CDE4B130-56B7-4231-A31A-76303DB23395}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
VersionInfoVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}
DefaultDirName={autopf}\{#AppFileName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; An upgrade from SessionStation would otherwise keep the old "SessionStation" Start menu folder.
UsePreviousGroup=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFilename={#AppFileName}-v{#AppVersion}-windows-x64-setup
SetupIconFile=..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2
SolidCompression=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"; AppUserModelID: "{#AppUserModelId}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; AppUserModelID: "{#AppUserModelId}"; Tasks: desktopicon

[InstallDelete]
; Upgrade from SessionStation (same AppId). The old script was per-user
; (PrivilegesRequired=lowest) with DefaultGroupName=SessionStation, so Inno resolved
; {group} to {userprograms}\SessionStation and {autodesktop} to {userdesktop}.
Type: files; Name: "{app}\session-station.exe"
Type: files; Name: "{userprograms}\SessionStation\SessionStation.lnk"
Type: files; Name: "{userdesktop}\SessionStation.lnk"
Type: dirifempty; Name: "{userprograms}\SessionStation"

[Registry]
; The app registers its toast identity here at runtime; remove it on uninstall.
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\{#AppUserModelId}"; Flags: uninsdeletekey dontcreatekey

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
const
  WV2_GUID = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

function WebView2Version(RootKey: Integer): String;
var
  pv: String;
begin
  Result := '';
  if RegQueryStringValue(RootKey, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WV2_GUID, 'pv', pv) then
    if (pv <> '') and (pv <> '0.0.0.0') then
      Result := pv;
end;

function IsWebView2Installed(): Boolean;
var
  found: String;
begin
  { WebView2 writes its version to the 32-bit registry view. }
  found := WebView2Version(HKLM32);
  if found = '' then
    found := WebView2Version(HKCU);
  if (found = '') and IsWin64 then
    found := WebView2Version(HKLM64);
  Result := found <> '';
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
  if not IsWebView2Installed() then
    if SuppressibleMsgBox('AI C-Station needs the Microsoft Edge WebView2 Runtime, which was not found.'#13#10#13#10 +
              'Install it for free from https://developer.microsoft.com/microsoft-edge/webview2/'#13#10#13#10 +
              'Continue anyway?', mbConfirmation, MB_YESNO, IDYES) = IDNO then
      Result := False;
end;
