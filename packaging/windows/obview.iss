; Installateur Windows d'Obview 3D (Inno Setup 6.6 ou plus récent ; D-32).
;
;   iscc /DAppVersion=0.2.1 /DSourceExe=<obview.exe> /DLicDir=<dossier des licences> /DOutputDir=<sortie> obview.iss
;
; Installation par utilisateur sans droits d'administrateur (choix « tous les utilisateurs » proposé), assistant
; sombre, raccourci du menu Démarrer, « Ouvrir avec » pour les formats lus, désinstallation. La mise à jour depuis
; l'application relance cet installateur avec /SILENT /RELAUNCH=1 (et /ALLUSERS ou /CURRENTUSER).

#define AppName "Obview 3D"
#define AppExe "Obview 3D.exe"
#define AppUrl "https://github.com/matv-studio/Obview-3D"
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\..\target\release\obview.exe"
#endif
#ifndef LicDir
  #define LicDir "licences"
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
AppId={{2F3678D1-E4B8-45D3-B2CC-E973F28D7B46}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=matv-studio
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=auto
DisableReadyPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
WizardStyle=modern dark
WizardSizePercent=110
SetupIconFile=..\..\apps\obview\assets\obview.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
OutputDir={#OutputDir}
OutputBaseFilename=Obview-3D-{#AppVersion}-windows-x64-setup
Compression=lzma2/ultra64
SolidCompression=yes
CloseApplications=force
RestartApplications=no
ChangesAssociations=yes
ShowLanguageDialog=auto

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion
Source: "{#LicDir}\*"; DestDir: "{app}\licences"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; Type de fichier « Modèle 3D » d'Obview, proposé dans « Ouvrir avec » sans prendre l'association par défaut.
Root: HKA; Subkey: "Software\Classes\Obview3D.Model"; ValueType: string; ValueName: ""; ValueData: "3D model"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Obview3D.Model\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe},0"
Root: HKA; Subkey: "Software\Classes\Obview3D.Model\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\.step\OpenWithProgids"; ValueType: string; ValueName: "Obview3D.Model"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".step"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\.stp\OpenWithProgids"; ValueType: string; ValueName: "Obview3D.Model"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".stp"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\.iges\OpenWithProgids"; ValueType: string; ValueName: "Obview3D.Model"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".iges"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\.igs\OpenWithProgids"; ValueType: string; ValueName: "Obview3D.Model"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".igs"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\.stl\OpenWithProgids"; ValueType: string; ValueName: "Obview3D.Model"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".stl"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\.obj\OpenWithProgids"; ValueType: string; ValueName: "Obview3D.Model"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".obj"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\.glb\OpenWithProgids"; ValueType: string; ValueName: "Obview3D.Model"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".glb"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\.gltf\OpenWithProgids"; ValueType: string; ValueName: "Obview3D.Model"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".gltf"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\.fbx\OpenWithProgids"; ValueType: string; ValueName: "Obview3D.Model"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".fbx"; ValueData: ""

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
; Mise à jour depuis l'application : relance après une installation silencieuse.
Filename: "{app}\{#AppExe}"; Flags: nowait runasoriginaluser; Check: Relaunch

[Code]
function Relaunch: Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:RELAUNCH|0}') = '1');
end;
