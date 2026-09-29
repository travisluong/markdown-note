; Compile with Inno Setup 6.3 or newer. See README.md in this directory.
#ifndef AppVersion
  #error AppVersion must be supplied from Cargo package metadata.
#endif
#ifndef SourceDir
  #error SourceDir must point to the directory containing markdown-note.exe.
#endif
#ifndef OutputDir
  #error OutputDir must be supplied.
#endif

[Setup]
; Keep this ID stable across versions so upgrades share one uninstall entry.
AppId={{1FDA4A23-D4A8-444F-8D03-40FE139443CE}
AppName=Markdown Note
AppVersion={#AppVersion}
AppPublisher=Travis Luong
AppPublisherURL=https://github.com/travisluong/markdown-note
AppSupportURL=https://github.com/travisluong/markdown-note/issues
AppUpdatesURL=https://github.com/travisluong/markdown-note/releases
DefaultDirName={localappdata}\Programs\Markdown Note
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableProgramGroupPage=yes
DisableDirPage=auto
WizardStyle=modern
SetupIconFile=..\..\assets\brand\markdown-note.ico
OutputDir={#OutputDir}
OutputBaseFilename=markdown-note-{#AppVersion}-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\markdown-note.exe
; Let users save and close the editor themselves before replacing locked files.
CloseApplications=no
RestartApplications=no
SetupLogging=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; List payload files explicitly. Never remove notebook or preference directories.
Source: "{#SourceDir}\markdown-note.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Markdown Note"; Filename: "{app}\markdown-note.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Markdown Note"; Filename: "{app}\markdown-note.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\markdown-note.exe"; Description: "Launch Markdown Note"; Flags: nowait postinstall skipifsilent
