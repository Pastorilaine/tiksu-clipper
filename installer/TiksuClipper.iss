; Per-user installer for Tiksu Clipper (no admin rights needed).
; Build: ISCC.exe /DAppVersion=0.3.0 installer\TiksuClipper.iss   (expects ..\TiksuClipper.exe, writes ..\TiksuClipper-Setup.exe)
; Installs to %LOCALAPPDATA%\Programs\TiksuClipper, a folder the app can write to, so its own
; signed auto-update keeps replacing TiksuClipper.exe in place.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{B7E1C2A4-5D3F-4A8E-9C61-7F2D0E4B8A13}
AppName=Tiksu Clipper
AppVersion={#AppVersion}
AppPublisher=IT-Veljekset Group
AppPublisherURL=https://github.com/Pastorilaine/tiksu-clipper
DefaultDirName={autopf}\TiksuClipper
DefaultGroupName=Tiksu Clipper
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..
OutputBaseFilename=TiksuClipper-Setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\TiksuClipper.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Tiksu Clipper asennusohjelma

[Languages]
Name: "finnish"; MessagesFile: "compiler:Languages\Finnish.isl"

[Tasks]
Name: "desktopicon"; Description: "Luo kuvake työpöydälle"; Flags: unchecked

[Files]
Source: "..\TiksuClipper.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Tiksu Clipper"; Filename: "{app}\TiksuClipper.exe"
Name: "{autodesktop}\Tiksu Clipper"; Filename: "{app}\TiksuClipper.exe"; Tasks: desktopicon

[Registry]
; The app's own "Käynnistä Windowsin mukana" setting writes this value; remove it on uninstall.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "TiksuClipper"; Flags: dontcreatekey uninsdeletevalue

[InstallDelete]
Type: files; Name: "{app}\TiksuClipper.exe.old"

[UninstallDelete]
; Left behind by an in-app update.
Type: files; Name: "{app}\TiksuClipper.exe.old"

[Run]
Filename: "{app}\TiksuClipper.exe"; Description: "Käynnistä Tiksu Clipper"; Flags: nowait postinstall skipifsilent

[Code]
// The app lives in the tray, so there may be no window to close: stop it before files are replaced.
procedure StopApp;
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM TiksuClipper.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Sleep(500);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopApp;
  Result := '';
end;

function InitializeUninstall: Boolean;
begin
  StopApp;
  Result := True;
end;
