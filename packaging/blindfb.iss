; BlindFB Windows installer, built by `cargo xtask package` (Inno Setup 6 or 7).
; Per-user: no administrator prompt, installs to %LOCALAPPDATA%\Programs\BlindFB,
; the folder the in-app updater already replaces files in.
; The version comes from the release binary, so there is nothing to bump here.

#define AppExe "..\target\release\blindfb.exe"
#define AppVersion GetStringFileInfo(AppExe, "ProductVersion")

[Setup]
; Never change the AppId: Windows uses it to find this install for upgrades and uninstall.
AppId={{AD8BE93E-6592-4281-B3C5-2FD5D6D5AFCE}
AppName=BlindFB
AppVersion={#AppVersion}
AppVerName=BlindFB {#AppVersion}
AppPublisher=serrebidev
AppPublisherURL=https://git.serrebiradio.com/serrebi/BlindFB
AppSupportURL=https://git.serrebiradio.com/serrebi/BlindFB/issues
AppUpdatesURL=https://git.serrebiradio.com/serrebi/BlindFB/releases
PrivilegesRequired=lowest
DefaultDirName={userpf}\BlindFB
DefaultGroupName=BlindFB
DisableProgramGroupPage=yes
DisableDirPage=auto
UsePreviousAppDir=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=BlindFB-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\blindfb.exe
UninstallDisplayName=BlindFB
; BlindFB keeps running in the tray after its window closes: ask it to close before
; replacing blindfb.exe instead of failing on a locked file.
CloseApplications=yes
RestartApplications=no
ShowLanguageDialog=auto

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\BlindFB"; Filename: "{app}\blindfb.exe"
Name: "{autodesktop}\BlindFB"; Filename: "{app}\blindfb.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\blindfb.exe"; Description: "{cm:LaunchProgram,BlindFB}"; Flags: nowait postinstall skipifsilent

; Settings, the browser profile and the backend live in %LOCALAPPDATA%\BlindFB, not here:
; uninstalling removes the program only, so a reinstall keeps the Facebook login.
