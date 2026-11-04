; Muninn setup.exe (Inno Setup 6.3+). Per-user: no admin prompt, installs to
; %LOCALAPPDATA%\Programs\Muninn, adds it to the user PATH (removed again on uninstall),
; a Start Menu entry that opens mnnn in Windows Terminal (or PowerShell), and an
; Add/Remove Programs entry. Built by scripts/windows-setup.sh:
;   ISCC /DAppVersion=2.20.1 /DArch=x86_64 /DSrcDir=<unzipped release folder> muninn.iss
#ifndef AppVersion
  #error Pass /DAppVersion=X.Y.Z (the Cargo.toml version)
#endif
#ifndef Arch
  #define Arch "x86_64"
#endif
#ifndef SrcDir
  #error Pass /DSrcDir=<folder with mnnn.exe, README.md, LICENSE ...>
#endif
#ifndef OutDir
  #define OutDir "."
#endif

[Setup]
AppId={{6A1C3E52-7B1F-4C3E-9D2A-4D5E6F708192}
AppName=Muninn
AppVersion={#AppVersion}
AppVerName=Muninn {#AppVersion}
AppPublisher=WeirdBrains
AppPublisherURL=https://muninn.weirdbrains.com
AppSupportURL=https://github.com/WeirdBrains/muninn/issues
AppUpdatesURL=https://github.com/WeirdBrains/muninn/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={userpf}\Muninn
DefaultGroupName=Muninn
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=
ChangesEnvironment=yes
LicenseFile={#SrcDir}\LICENSE
SetupIconFile=muninn.ico
UninstallDisplayIcon={app}\muninn.ico
UninstallDisplayName=Muninn {#AppVersion}
OutputDir={#OutDir}
OutputBaseFilename=muninn-{#AppVersion}-{#Arch}-windows-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
#if Arch == "aarch64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Files]
Source: "{#SrcDir}\mnnn.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SrcDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SrcDir}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SrcDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SrcDir}\LICENSE-FONTS"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "muninn.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "muninn-launch.cmd"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Muninn"; Filename: "{app}\muninn-launch.cmd"; WorkingDir: "{%USERPROFILE}"; IconFilename: "{app}\muninn.ico"; Flags: runminimized; Comment: "Muninn terminal UI"
Name: "{group}\Uninstall Muninn"; Filename: "{uninstallexe}"

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Check: NeedsAddPath(ExpandConstant('{app}'))

[Run]
Filename: "{app}\muninn-launch.cmd"; Description: "Open Muninn"; Flags: postinstall nowait skipifsilent runminimized

[Code]
function NeedsAddPath(Dir: string): Boolean;
var
  Path: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path) then
    Path := '';
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';') = 0;
end;

procedure RemovePath(Dir: string);
var
  Path, Upper, Needle: string;
  P: Integer;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path) then
    exit;
  Path := ';' + Path + ';';
  Needle := ';' + Uppercase(Dir) + ';';
  Upper := Uppercase(Path);
  P := Pos(Needle, Upper);
  while P > 0 do
  begin
    Delete(Path, P, Length(Needle) - 1);
    Upper := Uppercase(Path);
    P := Pos(Needle, Upper);
  end;
  while (Length(Path) > 0) and (Path[1] = ';') do
    Delete(Path, 1, 1);
  while (Length(Path) > 0) and (Path[Length(Path)] = ';') do
    Delete(Path, Length(Path), 1);
  RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemovePath(ExpandConstant('{app}'));
end;
