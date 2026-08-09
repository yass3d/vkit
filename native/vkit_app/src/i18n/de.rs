use super::TextKey;

pub(super) const fn label(key: TextKey) -> Option<&'static str> {
    #[allow(unreachable_patterns)]
    match key {
        TextKey::SurfaceSmooth => Some("Mesh-Glättung"),
        TextKey::SurfaceSmoothTooltip => Some(
            "VaM-kompatible Render-Glättung. 0 ist das bearbeitbare Basismesh, 3 der VaM-Standard, 4 das Maximum. Morphs, UVs, Sculpting und Textur-Baking behalten die Basistopologie",
        ),
        TextKey::FaceMatch => Some("Erstellen"),
        TextKey::ResultPreview => Some("Speichern"),
        TextKey::Save => Some("Speichern"),
        TextKey::DetailCorrection => Some("Sculpting"),
        TextKey::TextureStage => Some("Textur"),
        TextKey::MorphCategoryBody => Some("Körper"),
        TextKey::TextureNamePlaceholder => Some("Texturnamen eingeben"),
        TextKey::TextureOverwriteTitle => Some("Texturen mit gleichem Namen werden ersetzt"),
        TextKey::OpenScan => Some("Scan-Kopf"),
        TextKey::SourceMorphMissing => Some("Kein Morph dieses Looks ist installiert"),
        TextKey::EditDetails => Some("Details bearbeiten"),
        TextKey::Female => Some("Weiblich"),
        TextKey::Male => Some("Männlich"),
        TextKey::Lighting => Some("Beleuchtung"),
        TextKey::Brightness => Some("Helligkeit"),
        TextKey::LightRotation => Some("Lichtdrehung"),
        TextKey::AddHeadFileHint => Some("oder eine OBJ-, GLB- oder FBX-Datei hierher ziehen"),
        TextKey::AddHeadFile => Some("Kopfdatei hinzufügen"),
        TextKey::AutoAlign => Some("Auto-Ausrichten"),
        TextKey::PlacementReset => Some("Platzierung zurücksetzen"),
        TextKey::Scale => Some("Skalierung"),
        TextKey::Position => Some("Position"),
        TextKey::Rotation => Some("Rotation"),
        TextKey::XMirror => Some("X-Spiegelung"),
        TextKey::XMirrorTooltip => {
            Some("Ein gesetzter Pin setzt zugleich sein Gegenstück auf der anderen Seite")
        }
        TextKey::NumbersTooltip => {
            Some("Nummeriert jeden Pin in der Reihenfolge seiner Platzierung")
        }
        TextKey::XMirrorRejected => {
            Some("X-Spiegelung deaktiviert: kein sicheres Gegenstück gefunden")
        }
        TextKey::ScanFidelity => Some("Scan-Treue"),
        TextKey::ScanFidelityTooltip => Some(
            "Über 1 folgt die Anpassung stärker der Form des Scans und behält weniger von der Glätte der Figur. Bis 2 ist das alles; ab 2 schlägt auch das Rauschen des Scans -- Sprenkel, Löcher, Nahtstellen -- als Beulen durch, was sich bei einem sauberen, dichten Scan durchaus lohnen kann. Pins werden bei keiner Einstellung geopfert, und keine Schutzgrenze wird gelockert.",
        ),
        TextKey::ScanSymmetry => Some("Scan-Symmetrie"),
        TextKey::Original => Some("Original"),
        TextKey::Left => Some("Links"),
        TextKey::Right => Some("Rechts"),
        TextKey::EyesOpen => Some("Augen offen"),
        TextKey::EyesClosed => Some("Augen geschlossen"),
        TextKey::View => Some("Ansicht"),
        TextKey::On => Some("An"),
        TextKey::Numbers => Some("Nummern"),
        TextKey::PinPairsPlaced => Some("Pins"),
        TextKey::PinPlacement => Some("Pins setzen"),
        TextKey::ScanReference => Some("Scan-Referenz"),
        TextKey::AddHeadFileAction => Some("Kopfdatei hinzufügen"),
        TextKey::AddHeadFileActionTooltip => {
            Some("Eine gescannte Kopfdatei laden (OBJ · GLB · FBX)")
        }
        TextKey::ContinueStep => Some("Mit dem Basiskopf fortfahren"),
        TextKey::ContinueStepTooltip => Some(
            "Den G2-Kopf ohne Scan so bearbeiten, wie er ist; ein Scan lässt sich später laden und anpassen",
        ),
        TextKey::Undo => Some("Rückgängig"),
        TextKey::ResetAllPins => Some("Alle Pins zurücksetzen"),
        TextKey::MorphSave => Some("Morph speichern"),
        TextKey::TextureSaveSection => Some("Texturen speichern"),
        TextKey::Generate => Some("Gesicht anpassen"),
        TextKey::Cancel => Some("Abbrechen"),
        TextKey::CopyAll => Some("Alles kopieren"),
        TextKey::Next => Some("Speichern"),
        TextKey::TextureSkin => Some("Preset"),
        TextKey::SolidColor => Some("Einfarbig"),
        TextKey::BackfaceProtectionTooltip => Some("Schützt die Rückseiten dünner Meshes"),
        TextKey::WireframeColor => Some("Drahtgitterfarbe"),
        TextKey::WireframeSettings => Some("Drahtgitter"),
        TextKey::XraySettings => Some("Röntgen"),
        TextKey::ViewportLightingTooltip => Some("Beleuchtung anpassen"),
        TextKey::Background => Some("Hintergrund"),
        TextKey::ViewportBackgroundTooltip => Some("Hintergrund und Referenzbild anpassen"),
        TextKey::BackgroundRadial => Some("Radial"),
        TextKey::BackgroundVertical => Some("Vertikal"),
        TextKey::BackgroundFlat => Some("Einfarbig"),
        TextKey::ViewportWireframeTooltip => Some("Drahtgitter-Overlay anzeigen und anpassen"),
        TextKey::ViewportXrayTooltip => Some("Röntgen-Overlay anzeigen und anpassen"),
        TextKey::ViewportSkinTooltip => Some("Eine VaM-Hauttextur wählen"),
        TextKey::ViewportHairTooltip => Some("Ein VaM-Haar-Preset wählen"),
        TextKey::FalloffTooltip => Some("Die Einflusskurve des Pinsels wählen"),
        TextKey::FalloffSmooth => Some("Glatt"),
        TextKey::FalloffSmoother => Some("Glatter"),
        TextKey::FalloffSharp => Some("Scharf"),
        TextKey::FalloffLinear => Some("Linear"),
        TextKey::PlaceHead => Some("Kopf platzieren"),
        TextKey::PlaceHeadTooltip => {
            Some("Legt den geladenen Kopf über G2, um Position, Rotation und Größe auszurichten.")
        }
        TextKey::ScanOverlay => Some("Scan-Overlay"),
        TextKey::OverlayDone => Some("Overlay beenden"),
        TextKey::ProjectImage => Some("Projizieren"),
        TextKey::ProjectionCanvasHint => {
            Some("In der 3D-Ansicht malen — das Ergebnis landet auf dieser UV-Leinwand")
        }
        TextKey::HelpProjectionPaint => Some("Malen (projizieren)"),
        TextKey::ShortcutProjectionPaint => Some("Links ziehen"),
        TextKey::HelpProjectionMove => Some("Bild verschieben"),
        TextKey::ShortcutProjectionMove => Some("Rechts ziehen"),
        TextKey::HelpProjectionRotate => Some("Bild drehen"),
        TextKey::ShortcutProjectionRotate => Some("Umschalt + rechts ziehen"),
        TextKey::HelpProjectionZoom => Some("Bild skalieren"),
        TextKey::ShortcutProjectionZoom => Some("Mausrad"),
        TextKey::HelpBrushSize => Some("Pinselgröße"),
        TextKey::ShortcutBrushSize => Some("F ziehen · [ ]"),
        TextKey::HelpBrushStrength => Some("Pinselstärke"),
        TextKey::ShortcutBrushStrength => Some("Shift+F ziehen"),
        TextKey::HelpProjectionDone => Some("Projektion beenden"),
        TextKey::ShortcutProjectionDone => Some("Fertig · Esc"),
        TextKey::ProjectDone => Some("Fertig"),
        TextKey::ProjectDoneTooltip => {
            Some("Beendet die Projektion; alles Gemalte bleibt auf der Ebene")
        }
        TextKey::MorphCompare => Some("Vergleich"),
        TextKey::Opacity => Some("Deckkraft"),
        TextKey::G2Head => Some("G2-Kopf"),
        TextKey::Eyelashes => Some("Wimpern"),
        TextKey::OverwriteTitle => Some("Datei überschreiben"),
        TextKey::OverwriteBody => {
            Some("Eine Datei gleichen Namens existiert bereits. Überschreiben?")
        }
        TextKey::Overwrite => Some("Überschreiben"),
        TextKey::ScanTextureLayer => Some("Scan-Textur"),
        TextKey::PinPrompt => Some("Setzen Sie je einen Pin, oder fitten Sie ohne"),
        TextKey::TextureNeedsImage => Some("Fügen Sie über die Seitenleiste ein Bild hinzu"),
        TextKey::TexturePinPairPrompt => Some("Setzen Sie links und rechts zusammengehörige Pins"),
        TextKey::SymmetrySuggestion => {
            Some("Für einen saubereren Abgleich wird Spiegeln empfohlen")
        }
        TextKey::SymmetryChangeTitle => Some("Die Pins werden zurückgesetzt"),
        TextKey::SymmetryChangeConfirm => Some("Bestätigen"),
        TextKey::EyeClosure => Some("Augen schließen"),
        TextKey::VaMFolder => Some("VaM-Ordner"),
        #[cfg(test)]
        TextKey::VaMGeometryBaseReady => Some("VaM-Geometriebasis geprüft"),
        TextKey::VaMGeometryBaseRejected => Some("VaM-Geometriebasis nicht verwendbar"),
        TextKey::VaMGeometryBaseHowTo => Some(
            "VaM-Ordner wählen; die neutrale G2-Basis wird automatisch aus der Installation extrahiert",
        ),
        TextKey::VaMBaseSourceUnityBundle => {
            Some("VaM-interne neutrale Basis (automatisch extrahiert)")
        }
        TextKey::VaMBaseSourceDazAnchorPair => Some("DAZ-Anker + geprüfte OBJ-Basis"),
        TextKey::VaMBaseSourceUnityAnchorPair => {
            Some("Extrahierter neutraler Anker + geprüfte OBJ-Basis")
        }
        TextKey::VaMBaseSourceUnverifiedObj => Some("Ungeprüfte OBJ-Basis"),
        TextKey::VaMUvMappingUnavailable => Some(
            "Das VaM-Haut-UV-Mapping ist nicht bereit. VaM-Ordner prüfen und den Hautkatalog aktualisieren",
        ),
        TextKey::VaMIndexing => Some("VaM-Katalog wird indiziert"),
        TextKey::VaMFailed => Some("VaM-Katalog fehlgeschlagen"),
        TextKey::RefreshSkins => Some("Hautbibliothek aktualisieren"),
        TextKey::MorphLoading => Some("Morph wird geladen"),
        TextKey::MorphLoadFailed => Some("Morph konnte nicht geladen werden"),
        TextKey::DefaultSkinMissing => Some("Diese Haut wurde nicht gefunden. VaM-Ordner prüfen"),
        TextKey::DefaultSkinSet => Some("Mit dieser Haut öffnen"),
        TextKey::DefaultSkinClear => Some("Nicht mehr mit dieser Haut öffnen"),
        TextKey::SkinNone => Some("Keine"),
        TextKey::SkinSearch => Some("Haut suchen"),
        TextKey::SkinLoading => Some("Haut wird geladen"),
        TextKey::SkinReady => Some("Hautvorschau bereit"),
        TextKey::SkinLoadFailed => Some("Haut konnte nicht geladen werden"),
        TextKey::SkinUvUnavailable => Some("Haut-UV nicht verfügbar"),
        TextKey::NoMatchingSkins => Some("Keine passenden Haut-Presets"),
        TextKey::Hair => Some("Haar"),
        TextKey::HairNone => Some("Keins"),
        TextKey::HairSearch => Some("Haar suchen"),
        TextKey::HairLoading => Some("Haar wird geladen"),
        TextKey::HairReady => Some("Haarvorschau bereit"),
        TextKey::HairLoadFailed => Some("Haar konnte nicht geladen werden"),
        TextKey::HairPartsSkipped => Some("Einige Teile werden nicht angezeigt"),
        TextKey::HairParts => Some("Teile"),
        TextKey::BaseFace => Some("Basisgesicht"),
        TextKey::Settings => Some("Einstellungen"),
        TextKey::SettingsGraphics => Some("Grafik"),
        TextKey::SettingsViewport => Some("Viewport"),
        TextKey::SettingsGeneral => Some("Allgemein"),
        TextKey::SettingsAbout => Some("Über"),
        TextKey::SettingsAboutBuild => Some("Build"),
        TextKey::SettingsAboutLicense => Some("Lizenz"),
        TextKey::SettingsAboutDiagnostics => Some("Diagnose"),
        TextKey::SettingsTooltipsTooltip => Some("Hilfe wie diese hier, die gerade erschienen ist"),
        TextKey::AboutTagline => Some("Ein Begleitwerkzeug zum Erstellen von VaM-Kopfmorphs."),
        TextKey::AboutNoAffiliation => Some(
            "Virt-A-Mate gehört MeshedVR, Genesis 2 gehört DAZ 3D. Vkit ist ein unabhängiges Werkzeug ohne Verbindung zu beiden.",
        ),
        TextKey::AboutNoWarranty => Some(
            "Ohne Gewähr, wie besehen. Was Sie damit erstellen und die Lizenz geladener Inhalte liegen in Ihrer Verantwortung.",
        ),
        TextKey::AboutSupport => Some("Unterstützen"),
        TextKey::AboutReportProblem => Some(
            "Vkit schreibt bei jedem Start ein Protokoll und bei einem Absturz eine eigene Datei. Öffnen Sie den Ordner und hängen Sie beide an die Meldung: ein Fehler, den sonst niemand sieht, ist ein Fehler, den niemand beheben kann.",
        ),
        TextKey::AboutOpenLogs => Some("Protokollordner öffnen"),
        TextKey::AboutReportIssue => Some("Problem melden"),
        TextKey::SettingsLightingGroup => Some("Beleuchtung"),
        TextKey::SettingsLightingPreset => Some("Preset"),
        TextKey::SettingsExposure => Some("Belichtung"),
        TextKey::SettingsGeometryGroup => Some("Geometrie"),
        TextKey::SettingsSmoothPasses => Some("Oberflächenglättung"),
        TextKey::SettingsSmoothPassesHint => Some(
            "Glättet nur die Anzeige, so wie VaM es tut. Die gespeicherte Form bleibt unverändert.",
        ),
        TextKey::SettingsBackgroundGroup => Some("Hintergrund"),
        TextKey::SettingsBackground => Some("Stil"),
        TextKey::SettingsOverlayGroup => Some("Overlays"),
        TextKey::SettingsWireframeColor => Some("Drahtgitterfarbe"),
        TextKey::SettingsWireframeOpacity => Some("Drahtgitter-Deckkraft"),
        TextKey::SettingsXrayOpacity => Some("Röntgen-Deckkraft"),
        TextKey::SettingsSolidColor => Some("Kopffarbe (einfarbig)"),
        TextKey::SettingsLanguageGroup => Some("Sprache"),
        TextKey::SettingsMorphNames => Some("Namen integrierter Morphs"),
        TextKey::SettingsMorphNamesTranslated => Some("Übersetzt"),
        TextKey::SettingsMorphNamesOriginal => Some("Englisches Original"),
        TextKey::SettingsLanguage => Some("Anzeigesprache"),
        TextKey::SettingsFoldersGroup => Some("Ordner"),
        TextKey::SettingsVaMRoot => Some("VaM-Installationspfad"),
        TextKey::HairVisible => Some("Haar anzeigen"),
        TextKey::HairNeedsFinishedHead => Some("Haare werden auf einem fertigen Kopf getragen."),
        TextKey::HairPreviewCaveat => Some("Haardetails weichen vom Original ab."),
        TextKey::NoMatchingHair => Some("Keine passenden Haar-Presets"),
        TextKey::MorphSearch => Some("Morphs suchen"),
        TextKey::MorphOneSidedFilter => Some("L/R"),
        TextKey::MorphOneSidedFilterShow => Some("Morphs zeigen, die nur eine Seite bewegen"),
        TextKey::MorphOneSidedFilterHide => Some("Morphs ausblenden, die nur eine Seite bewegen"),
        TextKey::AppearanceSearch => Some("Looks suchen"),
        TextKey::LookFind => Some("Look laden"),
        TextKey::CameraTrackballArmed => Some("Rollen aktiv · seitwärts ziehen zum Neigen · Klick oder R beendet"),
        TextKey::HelpTrackball => Some("Trackball-Drehung"),
        TextKey::ShortcutTrackball => Some("R"),
        TextKey::RecoveryTitle => Some("Die letzte Sitzung wurde unerwartet beendet"),
        TextKey::RecoveryBody => Some(
            "Die Morphs, Sculpting- und Texturebenen wurden kurz davor gesichert. Zurückholen?",
        ),
        TextKey::RecoveryRestore => Some("Wiederherstellen"),
        TextKey::RecoveryDiscard => Some("Neu beginnen"),
        TextKey::RecoveryRestored => Some("Vorherige Sitzung wiederhergestellt."),
        TextKey::RecoveryLookMissing => Some(
            "Der Look jener Sitzung ist nicht mehr installiert, daher konnte nichts wiederhergestellt werden.",
        ),
        TextKey::SettingsOcclusionGroup => Some("Ambient Occlusion"),
        TextKey::SettingsOcclusionIntensity => Some("Stärke"),
        TextKey::SettingsOcclusionRadius => Some("Reichweite"),
        TextKey::SettingsBloomGroup => Some("Bloom"),
        TextKey::SettingsBloomIntensity => Some("Intensität"),
        TextKey::SettingsBloomThreshold => Some("Schwelle"),
        TextKey::SettingsBloomSoftKnee => Some("Soft Knee"),
        TextKey::SettingsBloomRadius => Some("Radius"),
        TextKey::SettingsVignetteGroup => Some("Vignette"),
        TextKey::SettingsVignetteIntensity => Some("Stärke"),
        TextKey::SettingsVignetteSmoothness => Some("Weichheit"),
        TextKey::SettingsVignetteRoundness => Some("Rundheit"),
        TextKey::SculptNotCarried => Some("Das Sculpting konnte diesem Look nicht folgen"),
        TextKey::SettingsToneCurve => Some("Tonkurve"),
        TextKey::SettingsToneCurveTooltip => Some("Passt Schatten und Lichter an"),
        TextKey::SettingsOcclusionTooltip => {
            Some("Verschattet die Vertiefungen, wo Flächen aufeinandertreffen")
        }
        TextKey::SettingsVignetteTooltip => Some("Dunkelt die Bildränder ab"),
        TextKey::SettingsEffectEnabled => Some("Aktiv"),
        TextKey::SettingsInterfaceGroup => Some("Benutzeroberfläche"),
        TextKey::SettingsTooltips => Some("Tooltips anzeigen"),
        TextKey::SettingsResetGroup => Some("Zurücksetzen"),
        TextKey::SettingsClearCache => Some("Cache leeren"),
        TextKey::SettingsClearCacheTooltip => Some(
            "Extrahierte Basisdaten und Morph-Bänke, die bei Bedarf neu entstehen. Das Leeren kostet nur einen langsameren Start.",
        ),
        TextKey::SettingsResetAll => Some("Alle Einstellungen zurücksetzen und neu starten"),
        TextKey::SettingsGraphicsLighting => Some("Beleuchtung"),
        TextKey::SettingsGraphicsEffects => Some("Effekte"),
        TextKey::SoloViewHint => Some("Nur diesen Teil anzeigen"),
        TextKey::SoloViewRestoreHint => Some("Vorherige Ansicht zurückholen"),
        TextKey::ToneCurveFilmic => Some("Filmic"),
        TextKey::ToneCurveSoft => Some("Weich"),
        TextKey::LooksHiddenForOtherFigure => Some("Looks für die andere Figur sind ausgeblendet"),
        TextKey::LookBelongsToOtherFigure => Some("Dieser Look ist für die andere Figur gebaut"),
        TextKey::MorphEdit => Some("Morphs bearbeiten"),
        TextKey::VaMMorphPairImported => Some("VaM-Morphpaar importiert"),
        TextKey::VaMMorphPairRejected => Some("VaM-Morphpaar kann nicht importiert werden"),
        TextKey::MorphCategoryAll => Some("Alle"),
        TextKey::MorphCategoryEyes => Some("Augen"),
        TextKey::MorphCategoryBrows => Some("Brauen"),
        TextKey::MorphCategoryNose => Some("Nase"),
        TextKey::MorphCategoryMouth => Some("Mund"),
        TextKey::MorphCategoryJaw => Some("Kiefer"),
        TextKey::MorphCategoryCheeks => Some("Wangen"),
        TextKey::MorphCategoryEars => Some("Ohren"),
        TextKey::MorphCategoryHead => Some("Gesicht"),
        TextKey::MorphCategoryExpression => Some("Ausdruck"),
        TextKey::NoMatchingMorphs => Some("Keine passenden Morphs"),
        TextKey::ResetMorphs => Some("Alle zurücksetzen"),
        TextKey::UndoMorphReset => Some("Zurücksetzen rückgängig"),
        TextKey::LightRotationGesture => Some("Shift+Rechtsklick ziehen"),
        TextKey::UpdateAvailable => Some("Update"),
        TextKey::PackageFromFiles => Some("Dateien wählen"),
        TextKey::PackageMorphFiles => Some("Morphs"),
        TextKey::PackageTextureFiles => Some("Texturen"),
        TextKey::PackageListEmpty => Some("Noch nichts hinzugefügt"),
        TextKey::LightingStudio => Some("Studio"),
        TextKey::LightingSoft => Some("Weich"),
        TextKey::LightingDaylight => Some("Tageslicht"),
        TextKey::LightingWarm => Some("Warm"),
        TextKey::LightingGloss => Some("Glanzprüfung"),
        TextKey::LightingPortrait => Some("Porträt"),
        TextKey::ImportLoadingMesh => Some("Mesh wird geladen"),
        TextKey::ImportSimplifying => Some("Mesh wird vereinfacht"),
        TextKey::StageOptimizeHead => Some("Eigenen Kopf optimieren"),
        TextKey::StagePrepareInput => Some("Eingabe vorbereiten"),
        TextKey::StageAlignScan => Some("Scan ausrichten"),
        TextKey::StageFitShape => Some("G2-Form anpassen"),
        TextKey::StageValidate => Some("Struktur prüfen"),
        TextKey::StagePrepareResult => Some("Ergebnis vorbereiten"),
        TextKey::HeavyMeshNote => Some("etwa {count} Dreiecke -- mehr davon dauert laenger"),
        TextKey::TransformGroups => Some("Teile"),
        TextKey::CollapseTransformGroups => Some("Teile einklappen"),
        TextKey::ExpandTransformGroups => Some("Teile ausklappen"),
        TextKey::Skin => Some("Haut"),
        TextKey::SculptTear => Some("Tränenfilm"),
        TextKey::SculptLips => Some("Lippen"),
        TextKey::SculptTeethTongue => Some("Zähne · Zunge"),
        TextKey::SculptInnerMouth => Some("Mundinneres"),
        TextKey::SculptEyes => Some("Augen"),
        TextKey::Size => Some("Größe"),
        TextKey::SculptXSymmetry => Some("X-Symmetrie"),
        TextKey::SculptXSymmetryTooltip => Some("Wendet denselben Strich auf beide Seiten an"),
        TextKey::BackfaceProtection => Some("Rückseitenschutz"),
        TextKey::AutoGroup => Some("Auto-Gruppe"),
        TextKey::AutoGroupTooltip => Some("Verformt nur innerhalb einer Polygongruppe"),
        TextKey::EyeTrackingAuto => Some("Auto"),
        TextKey::EyeGazeFreeze => Some("Als Morph einfrieren"),
        TextKey::EyeGazeFreezeTooltip => Some(
            "Fixiert die aktuelle Blickrichtung in den Morphs, sodass der Speichern-Tab diese Augenpose exportiert",
        ),
        TextKey::Reset => Some("Zurücksetzen"),
        TextKey::SculptGrab => Some("Greifen"),
        TextKey::SculptSmooth => Some("Glätten"),
        TextKey::SculptPushPull => Some("Drücken · Ziehen"),
        TextKey::ShortcutSculptGrab => Some("Ziehen"),
        TextKey::ShortcutSculptSmooth => Some("Umschalt + Ziehen"),
        TextKey::ShortcutSculptPushPull => Some("Strg / Alt + Ziehen"),
        TextKey::ResetSculpt => Some("Verformung zurücksetzen"),
        #[cfg(test)]
        TextKey::SculptApplied => Some("Sculpting-Änderungen übernommen"),
        TextKey::SculptFailed => Some("Sculpting-Vorgang fehlgeschlagen"),
        TextKey::Ready => Some("Bereit"),
        TextKey::Busy => Some("Bearbeitung ist während der Generierung gesperrt"),
        TextKey::NeedScan => Some(
            "Zuerst einen Scan-Kopf als OBJ, GLB oder FBX öffnen oder mit einem bereits in VaM vorhandenen Look beginnen",
        ),
        TextKey::ScanLoadFailed => Some("Diese Kopf-Datei konnte nicht gelesen werden"),
        TextKey::NeedEyeMorph => {
            Some("Der integrierte Augen-Morph ist mit diesem G2 nicht kompatibel")
        }
        TextKey::ReviewEyePins => Some("Die roten Augenlid-Pins prüfen"),
        TextKey::ResultUnavailable => Some("Zuerst das Gesicht anpassen"),
        TextKey::MorphNotInLibrary => {
            Some("Dieser Morph ist noch nicht in der Bibliothek; der Katalog lädt noch.")
        }
        TextKey::SculptNeedsBaseHead => {
            Some("Zuerst im Erstellen-Tab mit dem Basiskopf fortfahren.")
        }
        TextKey::MorphUnavailable => Some("Kein kompatibler Ergebnis-Morph verfügbar"),
        TextKey::AlignmentPending => Some("Scan-Kopf laden und VaM-Ordner wählen"),
        TextKey::ScanLoaded => Some("Eigene Kopfdatei (OBJ, GLB oder FBX) geladen"),
        TextKey::TemplateLoaded => Some("G2 geladen"),
        TextKey::PinsReset => Some("Alle Pins wurden zurückgesetzt"),
        TextKey::ResultStale => Some("Das Ergebnis muss neu erzeugt werden"),
        TextKey::GenerationStarted => Some("Gesicht wird angepasst"),
        TextKey::Cancelling => Some("Gesichtsanpassung wird abgebrochen"),
        TextKey::GenerationComplete => Some("Gesichtsanpassung abgeschlossen"),
        TextKey::GenerationCancelled => Some("Gesichtsanpassung abgebrochen"),
        TextKey::GenerationFailed => Some("Gesichtsanpassung fehlgeschlagen"),
        TextKey::ExportStarted => Some("Export läuft"),
        TextKey::ExportComplete => Some("Export abgeschlossen"),
        TextKey::ExportFailed => Some("Export fehlgeschlagen"),
        TextKey::MorphNameHint => Some("Morphnamen eingeben"),
        TextKey::MorphGroupHint => Some("Gruppe  (z. B. Custom, Morphs/Morph Loader)"),
        TextKey::MorphRegionHint => Some("Region  (z. B. Custom, Morphs/Body)"),
        TextKey::DirectEntry => Some("Direkt eingeben"),
        TextKey::VaMMetadataPoseMorph => Some("Als Pose-Morph registrieren"),
        TextKey::VaMMetadataPoseMorphTooltip => {
            Some("Wird zusammen mit Ausdrücken und Posen behandelt")
        }
        TextKey::VaMMetadataBoneCorrection => Some("Skelettkorrektur"),
        TextKey::VaMMetadataBoneCorrectionTooltip => {
            Some("Zentriert die Pivots am umgeformten Skelett neu")
        }
        TextKey::NativeCorePending => Some("Die Integration des nativen Fitting-Kerns läuft"),
        TextKey::MorphPreviewUpdated => Some("Morphvorschau aktualisiert"),
        TextKey::MorphsReset => Some("Morphs zurückgesetzt"),
        TextKey::MorphsResetUndone => Some("Morph-Zurücksetzen rückgängig gemacht"),
        TextKey::MorphEditUndone => Some("Morph-Anpassung rückgängig gemacht"),
        TextKey::MorphApplied => Some("Morph angewendet"),
        TextKey::HelpPlace => Some("Pin setzen"),
        TextKey::HelpMove => Some("Pin verschieben"),
        TextKey::HelpDelete => Some("Pin entfernen"),
        TextKey::HelpXSymmetry => Some("Pin-X-Symmetrie"),
        TextKey::HelpSnapRotation => Some("Rotation einrasten"),
        TextKey::HelpDragZoom => Some("Zoom ziehen"),
        TextKey::HelpOrbit => Some("Orbit"),
        TextKey::HelpPan => Some("Schwenken"),
        TextKey::HelpZoom => Some("Zoom"),
        TextKey::HelpLight => Some("Licht drehen"),
        TextKey::HelpFrameView => Some("Kopf einpassen"),
        TextKey::HelpUndo => Some("Rückgängig"),
        TextKey::ShortcutPlace => Some("Linksklick"),
        TextKey::ShortcutMove => Some("Alt + Ziehen"),
        TextKey::ShortcutDelete => Some("Rechtsklick"),
        TextKey::ShortcutXSymmetry => Some("X"),
        TextKey::ShortcutSnapRotation => Some("Umschalt / Strg beim Drehen"),
        TextKey::ShortcutDragZoom => Some("Strg + mittlere Maustaste senkrecht ziehen"),
        TextKey::ShortcutOrbit => Some("Mittlere Maustaste ziehen"),
        TextKey::ShortcutPan => Some("Umschalt + mittlere Maustaste ziehen"),
        TextKey::ShortcutZoom => Some("Mausrad"),
        TextKey::ShortcutLight => Some("Umschalt + rechts ziehen"),
        TextKey::ShortcutFrameView => Some("F"),
        TextKey::HelpSnapView => Some("Ansicht einrasten"),
        TextKey::ShortcutSnapView => Some("Alt + Rad ziehen"),
        TextKey::HelpStandardViews => Some("Standardansichten"),
        TextKey::ShortcutStandardViews => Some("Ziffernblock 1 3 7 9"),
        TextKey::ShortcutUndo => Some("Strg+Z"),
        TextKey::TemplatePending => {
            Some("VaM-Ordner wählen, um die G2-Basis automatisch vorzubereiten")
        }
        TextKey::ResultEmpty => Some("Kein vorbereiteter Morph"),
        TextKey::ScaleLink => Some("Skalierung koppeln"),
        TextKey::ScaleLinkTooltip => Some("Koppelt die X/Y/Z-Skalierungswerte"),
        TextKey::CameraSettings => Some("Kameraeinstellungen"),
        TextKey::Perspective => Some("Perspektivisch"),
        TextKey::Orthographic => Some("Orthografisch"),
        TextKey::Fov => Some("FOV"),
        TextKey::ResetCamera => Some("Kamera zurücksetzen"),
        TextKey::WindowMinimize => Some("Minimieren"),
        TextKey::WindowMaximize => Some("Maximieren"),
        TextKey::WindowRestore => Some("Wiederherstellen"),
        TextKey::WindowClose => Some("Schließen"),
        TextKey::TooltipShow => Some("Einblenden"),
        TextKey::TooltipHide => Some("Ausblenden"),
        TextKey::TooltipLock => Some("Sperren"),
        TextKey::TooltipUnlock => Some("Entsperren"),
        TextKey::AdjustEyeGaze => Some("Blickrichtung anpassen"),
        TextKey::ScanLoading => Some("Scan-Kopf wird geladen"),
        TextKey::TemplateLoading => Some("G2-Vorlage wird geladen"),
        TextKey::ResultLoading => Some("Ergebnis-Mesh wird geladen"),
        TextKey::VaMMorphPairLoading => Some("VaM-Morphpaar wird geladen"),
        TextKey::SystemLog => Some("Systemprotokoll"),
        TextKey::Strength => Some("Stärke"),
        TextKey::StrengthShort => Some("Stärke"),
        TextKey::SculptBrushMove => Some("Greifen"),
        TextKey::SculptBrushSmooth => Some("Glätten"),
        TextKey::SculptBrushRestore => Some("Basisform"),
        TextKey::SculptBrushMoveTooltip => {
            Some("Zieht die Oberfläche mit dem Zeiger; die Stärke bestimmt, wie weit sie folgt")
        }
        TextKey::SculptBrushSmoothTooltip => Some("Glättet Oberflächendetails"),
        TextKey::SculptBrushRestoreTooltip => Some("Stellt die Basisform schrittweise wieder her"),
        TextKey::TextureLayers => Some("Texturebenen"),
        TextKey::PackageSection => Some("VAR exportieren"),
        TextKey::PackageCreatorHint => Some("Ersteller"),
        TextKey::PackageNameHint => Some("Paketname"),
        TextKey::PackageDescriptionHint => Some("Beschreibung"),
        TextKey::PackageCreditsHint => Some("Credits"),
        TextKey::PackageInstructionsHint => Some("Anleitung"),
        TextKey::PackageLinkHint => Some("Support-Link"),
        TextKey::PackageSave => Some("VAR speichern"),
        TextKey::PackageFromThisHead => Some("Aus diesem Kopf erzeugen"),
        TextKey::PackageAddMorphs => Some("Morphs hinzufügen"),
        TextKey::PackageAddTextures => Some("Texturen hinzufügen"),
        TextKey::PackageNeedsVamRestart => Some("VaM muss neu gestartet werden"),
        TextKey::PackageSavedTo => Some("Gespeichert unter"),
        TextKey::FieldRequired => Some("(erforderlich)"),
        TextKey::FieldOptional => Some("(optional)"),
        TextKey::PackageSaveTo => Some("Speichern unter"),
        TextKey::PackageNothingToPack => {
            Some("Nichts zu packen. Diesen Kopf einschalten oder eine Datei hinzufügen.")
        }
        TextKey::PackageVersionMarker => Some("V"),
        TextKey::PackageCreatorRequired => Some("Ein Erstellername ist erforderlich."),
        TextKey::PackageNameRequired => Some("Ein Paketname ist erforderlich."),
        TextKey::PackageReplaceTitle => Some("Ein VAR mit diesem Namen existiert bereits"),
        TextKey::PackageReplaceBody => {
            Some("Ersetzen? Szenen, die auf diese Version verweisen, lesen dann den neuen Inhalt.")
        }
        TextKey::LicenseByTooltip => Some("Namensnennung"),
        TextKey::LicenseBySaTooltip => {
            Some("Urheber nennen / Weitergabe unter gleichen Bedingungen")
        }
        TextKey::LicenseByNdTooltip => Some("Urheber nennen / keine Bearbeitungen"),
        TextKey::LicenseByNcTooltip => Some("Urheber nennen / nicht kommerziell"),
        TextKey::LicenseByNcSaTooltip => {
            Some("Urheber nennen / nicht kommerziell / Weitergabe unter gleichen Bedingungen")
        }
        TextKey::LicenseByNcNdTooltip => {
            Some("Urheber nennen / nicht kommerziell / keine Bearbeitungen")
        }
        TextKey::LicensePcEaTooltip => Some("Early Access für Unterstützer, keine Weitergabe"),
        TextKey::AddTextureLayer => Some("Bild hinzufügen"),
        TextKey::AddG2UvTextureLayer => Some("UV-Map hinzufügen"),
        TextKey::AddTextureLayerTooltip => {
            Some("Legt ein Gesichtsfoto auf den Kopf und bearbeitet es als Textur")
        }
        TextKey::AddG2UvTextureLayerTooltip => {
            Some("Fuegt eine G2-Face-Textur im eigenen UV-Layout hinzu und bearbeitet sie")
        }
        TextKey::AddTextureLayerHint => {
            Some("Mit der +-Schaltfläche ein Gesichtsbild oder eine G2-Textur hinzufügen.")
        }
        TextKey::BakingTextures => Some("Texturen werden gebacken…"),
        TextKey::BakeTexturesForSave => Some("Texturen zum Speichern backen"),
        TextKey::TextureMetalRough => Some("Metallic / Roughness"),
        TextKey::TextureGlossSmooth => Some("Glossiness / Smoothness"),
        TextKey::LayerOpacity => Some("Ebenendeckkraft"),
        TextKey::PinOpacity => Some("Pin-Deckkraft"),
        TextKey::TransferPins => Some("Pins übertragen"),
        TextKey::TransferPinsTooltip => {
            Some("Kopiert die gesetzten Pins auf die Bilder der anderen Ebenen.")
        }
        TextKey::DeleteLayer => Some("Ebene löschen"),
        TextKey::TextureNormalStrength => Some("Normal-Stärke"),
        TextKey::TextureInvertScalar => Some("Skalarwerte invertieren"),
        TextKey::BlendNormal => Some("Normal"),
        TextKey::BlendMultiply => Some("Multiplizieren"),
        TextKey::BlendScreen => Some("Screen"),
        TextKey::BlendOverlay => Some("Überlagern"),
        TextKey::MatchToneToLayerBelow => Some("Ton an untere Ebene angleichen"),
        TextKey::ImageMirror => Some("Links/rechts spiegeln"),
        TextKey::ImageMirrorOff => Some("Aus"),
        TextKey::ImageMirrorToLeft => Some("Nach links"),
        TextKey::ImageMirrorToRight => Some("Nach rechts"),
        TextKey::ImageAdjustments => Some("Bildanpassungen"),
        TextKey::Exposure => Some("Belichtung"),
        TextKey::Contrast => Some("Kontrast"),
        TextKey::Saturation => Some("Sättigung"),
        TextKey::Hue => Some("Farbton"),
        TextKey::Temperature => Some("Temperatur"),
        TextKey::TextureMaskPreviewTooltip => {
            Some("Zeigt die Maske der aktiven Ebene als halbtransparentes rotes Overlay")
        }
        TextKey::ClearLayerMask => Some("Ebenenmaske leeren"),
        TextKey::ResetSourceRetouch => Some("Quellbild-Retusche zurücksetzen"),
        TextKey::CloneSampleHint => Some("Alt-Klick auf das Quellbild wählt eine Klonquelle."),
        TextKey::BakeBase => Some("Backoptionen"),
        TextKey::TransparentOverlay => Some("Nur Ebenen backen"),
        TextKey::CurrentSkinBake => Some("Mit VaM-Haut backen"),
        TextKey::TransparentOverlayTooltip => {
            Some("Backt allein die Ebenenbilder, ohne das Haut-Preset")
        }
        TextKey::CurrentSkinBakeTooltip => Some("Backt die Ebenen über das Haut-Preset als Basis"),
        TextKey::SkinPresetRequired => {
            Some("Zuerst ein Haut-Preset in den Hauteinstellungen wählen")
        }
        TextKey::SkinSettings => Some("Hauteinstellungen"),
        TextKey::ScanHeadColor => Some("Scan-Kopf-Farbe"),
        TextKey::G2HeadColor => Some("G2-Kopf-Farbe"),
        TextKey::MorphFilterAll => Some("Alle"),
        TextKey::MorphFilterActive => Some("Aktiv"),
        TextKey::BrushDodge => Some("Abwedeln"),
        TextKey::BrushBurn => Some("Nachbelichten"),
        TextKey::BrushSaturate => Some("Sättigen"),
        TextKey::BrushDesaturate => Some("Entsättigen"),
        TextKey::ShowLayersOnly => Some("Nur Ebenenbilder"),
        TextKey::ShowLayersOnlyTooltip => {
            Some("Blendet die VaM-Haut aus und zeigt nur die Ebenenbilder")
        }
        TextKey::MatchToneTooltip => {
            Some("Belichtung, Sättigung und Temperatur an die Ebene darunter angleichen")
        }
        TextKey::ClearLayerMaskTooltip => Some("Löscht die Maske dieser Ebene"),
        TextKey::TextureMetalRoughTooltip => {
            Some("Speichert die Maps in der Metallic/Roughness-Konvention")
        }
        TextKey::TextureGlossSmoothTooltip => {
            Some("Speichert die Maps in der Glossiness/Smoothness-Konvention von VaM")
        }
        TextKey::TextureInvertScalarTooltip => Some(
            "Kehrt die Werte um, wenn die Quelle die Gegenkonvention nutzt (Roughness ↔ Glossiness)",
        ),
        TextKey::BaseWithoutSkin => Some("Ebene"),
        TextKey::BaseWithoutSkinTooltip => Some(
            "Die gemalten Ebenen auf der einfarbigen Fläche, ohne Preset; die exportierte Textur bleibt gleich",
        ),
        TextKey::SolidColorTooltip => Some("Nur die einfarbige Fläche, keine Textur"),
        TextKey::TextureSkinTooltip => Some("Die gemalten Ebenen über dem VaM-Haut-Preset"),
        TextKey::BoundaryFeather => Some("Gesichtsrand weichzeichnen"),
        TextKey::BoundaryFeatherTooltip => Some(
            "Nur Diffuse. Maps wie Normal und Rauheit lassen sich nicht über Alpha überblenden, ihre Kante bleibt daher hart",
        ),
        TextKey::Baking => Some("Backen…"),
        TextKey::TextureToolPinPair => {
            Some("Pin-Paar — passende Punkte auf 3D-Gesicht und Quellbild setzen")
        }
        TextKey::TextureToolMask => Some("Maske — malen zum Freilegen; Alt halten zum Verbergen"),
        TextKey::TextureToolClone => {
            Some("Klonstempel — Alt-Klick setzt die Quelle, dann malen zum Kopieren")
        }
        TextKey::TextureToolDodgeBurn => {
            Some("Abwedeln/Nachbelichten — aufhellen; Alt halten zum Abdunkeln")
        }
        TextKey::TextureToolSponge => {
            Some("Schwamm — Sättigung hinzufügen; Alt halten zum Entsättigen")
        }
        TextKey::SelectTextureLayer => Some("Eine Texturebene wählen."),
        TextKey::LoadingTextureImage => Some("Bild wird geladen…"),
        TextKey::ScanTextureAlignment => {
            Some("Die Scan-Textur übernimmt die angepasste 3D-Ausrichtung.")
        }
        TextKey::NoTextureImage => Some("Kein Bild."),
        TextKey::MorphSavedReloadFemale => {
            Some("Auf VaMs Female-Morphs-Reiter Reload Custom Morphs drücken, um zu aktualisieren.")
        }
        TextKey::MorphSavedReloadMale => {
            Some("Auf VaMs Male-Morphs-Reiter Reload Custom Morphs drücken, um zu aktualisieren.")
        }
        TextKey::BootStarting => Some("Vkit wird vorbereitet"),
        TextKey::BootFonts => Some("Schriften werden geladen"),
        TextKey::BootGraphics => Some("Grafik wird gestartet"),
        TextKey::BootSettings => Some("Einstellungen werden geladen"),
        TextKey::BootTemplate => Some("G2 wird geladen"),
        TextKey::BootWorkspace => Some("Arbeitsbereich wird vorbereitet"),
        TextKey::BootReady => Some("Wird geöffnet"),
        TextKey::StartupFailedTitle => Some("Vkit konnte nicht starten"),
        TextKey::StartupFailedGraphics => Some(
            "Vkit zeichnet mit DirectX 12, und die Grafik dieses Rechners konnte das nicht bereitstellen. Aktualisieren Sie den Grafiktreiber, oder starten Sie Vkit auf einem Rechner mit einer DirectX-12-fähigen GPU. Eine Remotedesktopsitzung oder eine virtuelle Maschine hat oft keine eigene DirectX-12-Anzeige.",
        ),
        TextKey::StartupFailedOther => {
            Some("Vkit hat den Start abgebrochen und konnte sein Fenster nicht öffnen.")
        }
        TextKey::StartupFailedLogHint => Some("Was schiefging, wurde hier festgehalten:"),
        TextKey::DialogOpenScan => Some("Kopf-OBJ, -GLB oder -FBX öffnen"),
        TextKey::DialogAddUvLayer => Some("G2-UV-Texturebene hinzufügen"),
        TextKey::DialogAddTextureLayer => Some("Texturebene hinzufügen"),
        TextKey::DialogSaveMorphPair => Some("VaM-Morph als VMI + VMB speichern"),
        TextKey::DialogChooseVamFolder => Some("Virt-A-Mate-Ordner wählen"),
        _ => None,
    }
}
