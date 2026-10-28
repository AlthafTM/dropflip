#ifndef AppVersion
  #error AppVersion must be supplied
#endif
#ifndef BundleDir
  #error BundleDir must be supplied
#endif
#ifndef ReleaseDir
  #error ReleaseDir must be supplied
#endif

[Setup]
AppId={{986B87CA-AC4D-4E6C-AB43-D338815A5129}
AppName=Dropflip
AppVersion={#AppVersion}
AppPublisher=AlthafTM
AppPublisherURL=https://github.com/AlthafTM/dropflip
AppSupportURL=https://github.com/AlthafTM/dropflip/issues
DefaultDirName={localappdata}\Programs\Dropflip
DefaultGroupName=Dropflip
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#ReleaseDir}
OutputBaseFilename=Dropflip-v{#AppVersion}-windows-x64-setup
SetupIconFile=..\assets\dropflip.ico
UninstallDisplayIcon={app}\Dropflip.exe
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
DisableProgramGroupPage=yes

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked
Name: startup; Description: "Start Dropflip when I sign in"; Flags: checkedonce

[Files]
Source: "{#BundleDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Dropflip"; Filename: "{app}\Dropflip.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Dropflip"; Filename: "{app}\Dropflip.exe"; WorkingDir: "{app}"; Tasks: desktopicon
Name: "{userstartup}\Dropflip"; Filename: "{app}\Dropflip.exe"; WorkingDir: "{app}"; Tasks: startup

[Run]
Filename: "{app}\Dropflip.exe"; Description: "Open Dropflip"; Flags: nowait postinstall skipifsilent
