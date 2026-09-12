; Compile with Inno Setup 6.3 or newer. See README.md in this directory.
#ifndef AppVersion
  #error AppVersion must be supplied from Cargo package metadata.
#endif
#ifndef SourceDir
  #error SourceDir must point to the directory containing gpui-note.exe.
#endif
#ifndef OutputDir
  #error OutputDir must be supplied.
#endif

[Setup]
; Keep this ID stable across versions so upgrades share one uninstall entry.
AppId={{43268376-A27A-49D0-9C98-C8DF48B437C5}
AppName=GPUI Note
AppVersion={#AppVersion}
AppPublisher=Travis Luong
AppPublisherURL=https://github.com/travisluong/gpui-note
AppSupportURL=https://github.com/travisluong/gpui-note/issues
AppUpdatesURL=https://github.com/travisluong/gpui-note/releases
DefaultDirName={localappdata}\Programs\GPUI Note
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableProgramGroupPage=yes
DisableDirPage=auto
WizardStyle=modern
OutputDir={#OutputDir}
OutputBaseFilename=gpui-note-{#AppVersion}-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\gpui-note.exe
; Let users save and close the editor themselves before replacing locked files.
CloseApplications=no
RestartApplications=no
SetupLogging=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; List payload files explicitly. Never remove notebook or preference directories.
Source: "{#SourceDir}\gpui-note.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\GPUI Note"; Filename: "{app}\gpui-note.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\GPUI Note"; Filename: "{app}\gpui-note.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\gpui-note.exe"; Description: "Launch GPUI Note"; Flags: nowait postinstall skipifsilent
