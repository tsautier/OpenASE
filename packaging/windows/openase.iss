; Inno Setup script for the OpenASE Windows installer.
;
; Build (from the repository root, after `cargo build --release` in rust/):
;   iscc /DAppVersion=0.9.1 packaging\windows\openase.iss
; Optional defines: /DSourceExe=<path to the exe>  /DOutputDir=<folder>
;                   /DOutputName=<setup file name without .exe>

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\..\rust\target\release\openase.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif
#ifndef OutputName
  #define OutputName "OpenASE-v" + AppVersion + "-windows-x64-setup"
#endif

[Setup]
; Fixed forever: this GUID is what makes a new setup upgrade an older install.
AppId={{5C725B6E-FEF6-47EC-94BB-279FCFB8786D}
AppName=OpenASE
AppVersion={#AppVersion}
AppVerName=OpenASE {#AppVersion}
AppPublisher=Thomas Sautier
AppPublisherURL=https://openase.org
AppSupportURL=https://github.com/tsautier/OpenASE/issues
AppUpdatesURL=https://github.com/tsautier/OpenASE/releases
DefaultDirName={autopf}\OpenASE
DefaultGroupName=OpenASE
DisableProgramGroupPage=yes
; Per-user by default (no admin prompt); the user can still choose "all users".
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE
SetupIconFile=..\..\rust\assets\icon.ico
UninstallDisplayIcon={app}\OpenASE.exe
UninstallDisplayName=OpenASE
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "OpenASE.exe"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\OpenASE"; Filename: "{app}\OpenASE.exe"
Name: "{autodesktop}\OpenASE"; Filename: "{app}\OpenASE.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\OpenASE.exe"; Description: "{cm:LaunchProgram,OpenASE}"; Flags: nowait postinstall skipifsilent

; Settings live in %APPDATA%\OpenASE. There is deliberately no [UninstallDelete]
; entry for it: uninstalling or upgrading keeps the user's settings.
