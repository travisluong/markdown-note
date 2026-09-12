; Compile with Inno Setup 6.3 or newer. See README.md in this directory.
#ifndef AppVersion
  #error AppVersion must be supplied from Cargo package metadata.
#endif
#ifndef SourceDir
  #error SourceDir must point to the directory containing mithril.exe.
#endif
#ifndef OutputDir
  #error OutputDir must be supplied.
#endif

[Setup]
; Keep this ID stable across versions so upgrades share one uninstall entry.
AppId={{43268376-A27A-49D0-9C98-C8DF48B437C5}
AppName=Mithril
AppVersion={#AppVersion}
AppPublisher=Travis Luong
AppPublisherURL=https://github.com/travisluong/mithril
AppSupportURL=https://github.com/travisluong/mithril/issues
AppUpdatesURL=https://github.com/travisluong/mithril-releases/releases
DefaultDirName={localappdata}\Programs\Mithril
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableProgramGroupPage=yes
DisableDirPage=auto
WizardStyle=modern
SetupIconFile=..\..\assets\brand\mithril.ico
OutputDir={#OutputDir}
OutputBaseFilename=mithril-{#AppVersion}-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\mithril.exe
; Let users save and close the editor themselves before replacing locked files.
CloseApplications=no
RestartApplications=no
SetupLogging=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; List payload files explicitly. Never remove notebook or preference directories.
Source: "{#SourceDir}\mithril.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Mithril"; Filename: "{app}\mithril.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Mithril"; Filename: "{app}\mithril.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\mithril.exe"; Description: "Launch Mithril"; Flags: nowait postinstall skipifsilent
