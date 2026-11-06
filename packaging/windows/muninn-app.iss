; Muninn App setup.exe (Inno Setup 6.3+) for the Flutter GUI.
; Separate from the TUI muninn.iss (AppId {{6A1C3E52-…}}, DefaultDirName {userpf}\Muninn).
; Per-user: no admin prompt, installs to %LOCALAPPDATA%\Programs\MuninnApp, optional
; user PATH entry for muninn.exe, Start Menu entry that launches the GUI.
; Built by scripts/flutter-dist.sh on Windows:
;   ISCC /DAppVersion=2.22.0 /DArch=x86_64 /DSrcDir=<flutter Release folder> muninn-app.iss
#ifndef AppVersion
  #error Pass /DAppVersion=X.Y.Z (the Cargo.toml / app version)
#endif
#ifndef Arch
  #define Arch "x86_64"
#endif
#ifndef SrcDir
  #error Pass /DSrcDir=<Flutter windows release runner folder with muninn.exe>
#endif
#ifndef OutDir
  #define OutDir "."
#endif
#ifndef IconFile
  #define IconFile "app_icon.ico"
#endif

[Setup]
AppId={{B7E2D941-5A6C-4F8E-9B1D-3C4A5E6F7082}
AppName=Muninn App
AppVersion={#AppVersion}
AppVerName=Muninn App {#AppVersion}
AppPublisher=WeirdBrains
AppPublisherURL=https://muninn.weirdbrains.com
AppSupportURL=https://github.com/WeirdBrains/muninn/issues
AppUpdatesURL=https://github.com/WeirdBrains/muninn/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\MuninnApp
DefaultGroupName=Muninn App
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=
ChangesEnvironment=yes
LicenseFile={#SrcDir}\LICENSE
SetupIconFile={#IconFile}
UninstallDisplayIcon={app}\muninn.exe
UninstallDisplayName=Muninn App {#AppVersion}
OutputDir={#OutDir}
OutputBaseFilename=muninn-app-{#AppVersion}-{#Arch}-windows-setup
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
; Flutter Windows release folder: muninn.exe, flutter_windows.dll, data\, *.dll, …
Source: "{#SrcDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
; LICENSE/CHANGELOG may already be in SrcDir when flutter-dist stages them; also allow
; skip if the release folder already shipped them via the copy step.
Source: "{#SrcDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#SrcDir}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#SrcDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\Muninn App"; Filename: "{app}\muninn.exe"; WorkingDir: "{app}"; Comment: "Muninn graphical workbench"
Name: "{group}\Uninstall Muninn App"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\muninn.exe"; Description: "Open Muninn App"; Flags: postinstall nowait skipifsilent

[Code]
// Same PATH helper pattern as muninn.iss (TUI): add {app} to HKCU Path on install,
// remove exactly that entry on uninstall.
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
    exit;
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
  if CurUninstallStep = usUninstall then
    RemovePath(ExpandConstant('{app}'));
end;
