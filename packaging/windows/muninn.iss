; Muninn setup.exe (Inno Setup 6.3+). Per-user: no admin prompt, installs to
; %LOCALAPPDATA%\Programs\Muninn, adds it to the user PATH (removed again on uninstall),
; a Start Menu entry that opens mnnn in Windows Terminal (or PowerShell), and an
; Add/Remove Programs entry. Built by scripts/windows-setup.sh:
;   ISCC /DAppVersion=2.23.0 /DArch=x86_64 /DSrcDir=<unzipped release folder> muninn.iss
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
#ifndef IconFile
  #define IconFile "muninn.ico"
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
SetupIconFile={#IconFile}
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
Source: "{#IconFile}"; DestDir: "{app}"; Flags: ignoreversion
Source: "muninn-launch.cmd"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Muninn"; Filename: "{app}\muninn-launch.cmd"; WorkingDir: "{%USERPROFILE}"; IconFilename: "{app}\muninn.ico"; Flags: runminimized; Comment: "Muninn terminal UI"
Name: "{group}\Uninstall Muninn"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\muninn-launch.cmd"; Description: "Open Muninn"; Flags: postinstall nowait skipifsilent runminimized

[Code]
// The user PATH (HKCU\Environment\Path, REG_EXPAND_SZ) gets the install folder once on
// install and loses exactly that entry on uninstall. Entries are compared without case,
// surrounding spaces, a trailing backslash or unexpanded %VARS%; every other entry, its
// spelling and its order are kept, and empty entries left by stray semicolons are dropped.
function Norm(Entry: string): string;
begin
  Result := Uppercase(Trim(Entry));
  StringChangeEx(Result, '/', '\', True);
  while (Length(Result) > 3) and (Result[Length(Result)] = '\') do
    Delete(Result, Length(Result), 1);
end;

function ExpandEnv(S: string): string;
var
  A, B: Integer;
  Name, Rest: string;
begin
  Result := '';
  Rest := S;
  A := Pos('%', Rest);
  while A > 0 do
  begin
    Result := Result + Copy(Rest, 1, A - 1);
    Rest := Copy(Rest, A + 1, Length(Rest));
    B := Pos('%', Rest);
    if B = 0 then
    begin
      Result := Result + '%';
      break;
    end;
    Name := Copy(Rest, 1, B - 1);
    if GetEnv(Name) <> '' then
      Result := Result + GetEnv(Name)
    else
      Result := Result + '%' + Name + '%';
    Rest := Copy(Rest, B + 1, Length(Rest));
    A := Pos('%', Rest);
  end;
  Result := Result + Rest;
end;

function Same(Entry, Dir: string): Boolean;
begin
  Result := Norm(ExpandEnv(Entry)) = Norm(Dir);
end;

// The PATH without Dir (Found: whether it was there), rebuilt from its other entries.
function Without(Path, Dir: string; var Found: Boolean): string;
var
  P: Integer;
  Entry, Rest: string;
begin
  Result := '';
  Found := False;
  Rest := Path + ';';
  P := Pos(';', Rest);
  while P > 0 do
  begin
    Entry := Copy(Rest, 1, P - 1);
    Rest := Copy(Rest, P + 1, Length(Rest));
    if Trim(Entry) <> '' then
    begin
      if Same(Entry, Dir) then
        Found := True
      else if Result = '' then
        Result := Entry
      else
        Result := Result + ';' + Entry;
    end;
    P := Pos(';', Rest);
  end;
end;

function UserPath(var Path: string): Boolean;
begin
  Result := RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path);
  if not Result then
    Path := '';
end;

procedure AddPath(Dir: string);
var
  Path: string;
  Found: Boolean;
begin
  UserPath(Path);
  Without(Path, Dir, Found);
  if Found then
    exit; // already there: leave the value exactly as it is
  while (Length(Path) > 0) and (Path[Length(Path)] = ';') do
    Delete(Path, Length(Path), 1);
  if Path = '' then
    Path := Dir
  else
    Path := Path + ';' + Dir;
  RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path);
end;

procedure RemovePath(Dir: string);
var
  Path, Kept: string;
  Found: Boolean;
begin
  if not UserPath(Path) then
    exit;
  Kept := Without(Path, Dir, Found);
  if not Found then
    exit;
  if Kept = '' then
    RegDeleteValue(HKEY_CURRENT_USER, 'Environment', 'Path')
  else
    RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Kept);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    AddPath(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  // usUninstall: the install folder is still known and its files are about to go
  if CurUninstallStep = usUninstall then
    RemovePath(ExpandConstant('{app}'));
end;
