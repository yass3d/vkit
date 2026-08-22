use super::TextKey;

pub(super) const fn label(key: TextKey) -> Option<&'static str> {
    #[allow(unreachable_patterns, reason = "the fallback outlives today's key set")]
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
        TextKey::HairStage => Some("Haar"),
        TextKey::HairUnavailable => {
            Some("Schließe zuerst das Sculpt-Ergebnis ab, bevor du Haare bearbeitest")
        }
        TextKey::HairPartsPanel => Some("Haarteile"),
        TextKey::AddHairPart => Some("Haarteil hinzufügen"),
        TextKey::HairScalpMissing => {
            Some("Kopfhaut-Mesh fehlt; öffne einen VaM-Ordner und scanne erneut")
        }
        TextKey::HairShowPoints => Some("Punkte anzeigen"),
        TextKey::HairShowPointsHint => Some(
            "Zeigt die Pflanzpunkte auf der Kopfhaut. Zum Ausblenden abschalten; die Pinsel arbeiten weiter.",
        ),
        TextKey::HairPartTint => Some("Teile einfärben"),
        TextKey::HairPartTintHint => Some(
            "Gibt jedem Teil eine eigene Tönung im Viewport, um sie zu unterscheiden. Nur Anzeige; der Export bleibt unberührt.",
        ),
        TextKey::HairSettingsPanel => Some("Haar-Einstellungen"),
        TextKey::HairCreateFirst => Some("Erstelle zuerst Haar."),
        TextKey::HairHideStrands => Some("Haar ausblenden"),
        TextKey::HairHideStrandsHint => Some(
            "Nimmt das gewachsene Haar weg, damit nur die Straehnen zu lesen sind. Einschalten schaltet die Straehnen mit ein.",
        ),
        TextKey::HairShowStreams => Some("Haarstraehnen zeigen"),
        TextKey::HairShowStreamsHint => {
            Some("Zeichnet die Straehne, die du bearbeitest, ueber dem Haar das daraus waechst.")
        }
        TextKey::HairViewportPhysics => Some("Viewport-Physik"),
        TextKey::HairViewportPhysicsHint => {
            Some("Zeigt die Physik auf dem Bildschirm. Hat nichts mit dem Export zu tun.")
        }
        TextKey::HairMirrorEdit => Some("Spiegel-Bearbeitung"),
        TextKey::HairMirrorEditHint => {
            Some("Pinsel bearbeiten beide Seiten zugleich, links-rechts symmetrisch.")
        }
        TextKey::HairAutoPart => Some("Auto-Part"),
        TextKey::HairAutoPartHint => Some(
            "Der Pinsel bearbeitet nur das unter dem Cursor erkannte Part statt der aktiven Ebenen.",
        ),
        TextKey::HairExportSection => Some("Haar speichern"),
        TextKey::HairExportBothSexes => Some("Beide"),
        TextKey::HairExportRescanNotice => {
            Some("VaM - Hair - Rescan Files anklicken, damit es in der Liste erscheint")
        }
        TextKey::HairExportOverwroteNotice => {
            Some("Ein überschriebenes Vorschaubild erscheint nach Hard Reset oder VaM-Neustart")
        }
        TextKey::HairSimToggleHint => Some("Fuehrt die Haarphysik hier und im Spiel aus."),
        TextKey::HairCollisionToggleHint => Some("Haelt Straehnen aus Kopf und Koerper heraus."),
        TextKey::HairStyleJoints => Some("Stil-Gelenke"),
        TextKey::HairStyleJointsHint => Some(
            "Verbindet nahe Strähnen, damit die Frisur im Spiel ihre Form hält -- das, was Cling skaliert. Für langes oder gebundenes Haar; macht die Datei schwerer.",
        ),
        TextKey::HairThumbnailPrompt => Some("Winkel einstellen und zum Aufnehmen klicken"),
        TextKey::HairThumbnailShoot => Some("Aufnehmen"),
        TextKey::HairThumbnailSkip => Some("Überspringen"),
        TextKey::HairThumbnailSaved => Some("Vorschaubild gespeichert"),
        TextKey::HairThumbnailFailed => Some("Vorschaubild fehlgeschlagen"),
        TextKey::HairGameOnly => Some(
            "Physik-Einstellung: das Viewport simuliert nicht, wirkt also nur in VaM. Wird mit dem Stil exportiert.",
        ),
        TextKey::HairPartVisible => Some("Dieses Haarteil ein-/ausblenden"),
        TextKey::RemoveHairPart => Some("Haarteil entfernen"),
        TextKey::HairRenamePart => Some("Zum Umbenennen klicken"),
        TextKey::HairToolPlant => Some("Pflanzen"),
        TextKey::HairToolErase => Some("Löschen"),
        TextKey::HairToolGrow => Some("Wachsen (Alt kürzt)"),
        TextKey::HairToolComb => Some("Kämmen"),
        TextKey::HairToolPlantHint => {
            Some("Straehnen auf den Kopfhautpunkten unter dem Pinsel wachsen lassen")
        }
        TextKey::HairToolGrowHint => {
            Some("Straehnen unter dem Pinsel verlaengern; Alt haelt sie kuerzer")
        }
        TextKey::HairToolEraseHint => Some("Straehnen unter dem Pinsel entfernen"),
        TextKey::HairToolCombHint => {
            Some("Straehnen unter dem Pinsel in Form ziehen; die Ansaetze bleiben")
        }
        TextKey::HairToolPinch => Some("Buendeln"),
        TextKey::HairToolPinchHint => {
            Some("Fasst die Straehnen unter dem Pinsel zusammen. Alt spreizt sie")
        }
        TextKey::HairToolCut => Some("Schneiden"),
        TextKey::HairToolCutHint => Some("Kuerzt die Straehnen dort, wo der Pinsel sie kreuzt"),
        TextKey::HairToolPuff => Some("Aufplustern"),
        TextKey::HairToolPuffHint => {
            Some("Stellt die Straehnen unter dem Pinsel von der Haut ab. Alt legt sie an")
        }
        TextKey::HairToolVertex => Some("Punkt"),
        TextKey::HairToolVertexHint => {
            Some("Ein Gelenk einer Strähne greifen und bewegen; die Länge bleibt erhalten")
        }
        TextKey::HairCopySettings => Some("Alles kopieren"),
        TextKey::HairPasteSettings => Some("Alles einfuegen"),
        TextKey::HairGroupPerformance => Some("Leistung"),
        TextKey::HairGroupPhysics => Some("Physik"),
        TextKey::HairGroupStiffness => Some("Steifigkeit"),
        TextKey::HairGroupShape => Some("Form"),
        TextKey::HairGroupCurl => Some("Locken"),
        TextKey::HairGroupLook => Some("Aussehen"),
        TextKey::HairColorScalp => Some("Kopfhaut"),
        TextKey::HairColorRoot => Some("Ansatz"),
        TextKey::HairColorTip => Some("Spitze"),
        TextKey::HairColorSpecular => Some("Glanz"),
        TextKey::HairScalpMask => Some("Kopfhaut-Textur"),
        TextKey::HairScalpMaskBuiltIn => Some("Integriert"),
        TextKey::HairScalpMaskCustom => Some("Aus Datei..."),
        TextKey::HairExport => Some("Nach VaM exportieren"),
        TextKey::HairExportName => Some("Elementname"),
        TextKey::HairExportCreator => Some("Ersteller"),
        TextKey::HairExportNeedsMetadata => {
            Some("Name und Ersteller ausfuellen, bevor exportiert wird")
        }
        TextKey::HairOverwriteTitle => Some("Diesen Stil ersetzen?"),
        TextKey::HairOverwriteBody => Some(
            "Ein Stil mit diesem Namen und Ersteller ist bereits installiert. Der Export ersetzt seine Dateien.",
        ),
        TextKey::HairOverwriteProceed => Some("Ersetzen"),
        TextKey::HairExportDone => Some("Haar-Element gespeichert"),
        TextKey::HairExportFailed => Some("Haar-Export fehlgeschlagen"),
        TextKey::HairExportNeedsPart => Some("Waehle zuerst ein Haarteil mit Straehnen"),
        TextKey::HairExportNeedsVaMRoot => Some("Setze vor dem Export einen VaM-Ordner"),
        TextKey::HairMirrorPart => Some("Teil auf die andere Seite spiegeln"),
        TextKey::HairDuplicatePart => Some("Teil duplizieren"),
        TextKey::HairSegmentsShort => Some("Seg"),
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
        TextKey::ScanOptions => Some("Scan-Optionen"),
        TextKey::RestoreNeckEars => Some("Hals & Ohren wiederherstellen"),
        TextKey::RestoreNeckEarsTooltip => Some(
            "Hält Ohren, Nacken und Hinterkopf auf der G2-Basis und blendet die Anpassung nur zum Gesicht hin ein",
        ),
        TextKey::RestoreNeckEarsDeferred => {
            Some("Es gibt Sculpt-Striche; die Änderung greift bei der nächsten Generierung")
        }
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
        TextKey::ResetAll => Some("Alles zuruecksetzen"),
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
        TextKey::TextureToolNeedsPins => Some("Erst die Pins setzen, dann nutzbar"),
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
        TextKey::HairSearch => Some("Haar suchen"),
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
        TextKey::SettingsShortcuts => Some("Tastenkürzel"),
        TextKey::ShortcutsCapturing => Some("Drücken…"),
        TextKey::ShortcutsResetAll => Some("Alle Tastenkürzel zurücksetzen"),
        TextKey::ShortcutsExport => Some("Belegung als Datei speichern"),
        TextKey::ShortcutsImport => Some("Belegungsdatei laden"),
        TextKey::ShortcutsTaken => Some("Eine andere Aktion hört darauf"),
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
        TextKey::HairNeedsFinishedHead => Some("Haare werden auf einem fertigen Kopf getragen."),
        TextKey::NoMatchingHair => Some("Keine passenden Haar-Presets"),
        TextKey::MorphSearch => Some("Morphs suchen"),
        TextKey::MorphOneSidedFilter => Some("L/R"),
        TextKey::MorphOneSidedFilterShow => Some("Morphs zeigen, die nur eine Seite bewegen"),
        TextKey::MorphOneSidedFilterHide => Some("Morphs ausblenden, die nur eine Seite bewegen"),
        TextKey::AppearanceSearch => Some("Looks suchen"),
        TextKey::AppearanceLayers => Some("Aussehens-Ebenen"),
        TextKey::AddAppearanceLayer => Some("Gewaehltes Aussehen hinzufuegen"),
        TextKey::AppearanceLayersEmpty => Some("Noch keine Ebenen"),
        TextKey::AppearanceLayerRaise => Some("Nach oben"),
        TextKey::AppearanceLayerLower => Some("Nach unten"),
        TextKey::LookFind => Some("Look laden"),
        TextKey::CameraTrackballArmed => {
            Some("Trackball · zum freien Drehen ziehen · Klick oder R zum Verlassen")
        }
        TextKey::HelpTrackball => Some("Trackball-Drehung"),
        TextKey::ShortcutTrackball => Some("R"),
        TextKey::SplitModelViewTooltip => Some("Aus zwei Winkeln sehen — Ansicht teilen"),
        TextKey::SplitModelViewName => Some("Geteilte Ansicht"),
        TextKey::HelpLevelRoll => Some("Horizont waagerecht stellen"),
        TextKey::ShortcutLevelRoll => Some("Alt+R"),
        TextKey::RecoveryTitle => Some("Die letzte Sitzung wurde unerwartet beendet"),
        TextKey::RecoveryBody => Some(
            "Die Morphs, Sculpting- und Texturebenen wurden kurz davor gesichert. Zurückholen?",
        ),
        TextKey::RecoveryRestore => Some("Wiederherstellen"),
        TextKey::RecoveryDiscard => Some("Neu beginnen"),
        TextKey::RecoveryRestored => Some("Vorherige Sitzung wiederhergestellt."),
        TextKey::RecoveryLookMissing => Some(
            "Der Look dieser Sitzung ist noch nicht gelistet — Texturen wurden wiederhergestellt; die Änderungen greifen, sobald der Look gewählt wird.",
        ),
        TextKey::SettingsVignetteGroup => Some("Vignette"),
        TextKey::SettingsBrushSweepGroup => Some("Pinselgrößen-Geste"),
        TextKey::BrushSweepBlender => Some("Blender"),
        TextKey::BrushSweepZBrush => Some("ZBrush"),
        TextKey::BrushSweepTooltip => {
            Some("Blender bestätigt per Klick oder zweitem Druck; ZBrush beim Loslassen")
        }
        TextKey::HairPackageStyle => Some("Als .var paketieren"),
        TextKey::HairPackageDone => Some("Haar paketiert"),
        TextKey::HairPackageFailed => Some("Paketieren fehlgeschlagen"),
        TextKey::HairPackageNeedsInstall => Some("Erst die Frisur installieren"),
        TextKey::DialogOpenHeadPreset => Some("Kopf-Preset öffnen"),
        TextKey::FindHeadPreset => Some("Preset suchen…"),
        TextKey::SettingsVignetteIntensity => Some("Stärke"),
        TextKey::SettingsVignetteSmoothness => Some("Weichheit"),
        TextKey::SettingsVignetteRoundness => Some("Rundheit"),
        TextKey::SculptNotCarried => Some("Das Sculpting konnte diesem Look nicht folgen"),
        TextKey::SettingsToneCurve => Some("Tonkurve"),
        TextKey::SettingsToneCurveTooltip => Some("Passt Schatten und Lichter an"),
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
        TextKey::ScanUnloaded => Some("Kopfdatei entfernt; die G2-Basis ist bereit"),
        TextKey::PinPairsMismatched => Some("Pin-Anzahl stimmt nicht überein; Pins ohne Partner"),
        TextKey::UnloadScanTooltip => Some("Diese Kopfdatei entfernen"),
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
        TextKey::HelpCameraProjection => Some("Perspektive / Ortho"),
        TextKey::ShortcutStandardViews => Some("Ziffernblock 1-9"),
        TextKey::ShortcutCameraProjection => Some("Ziffernblock ."),
        TextKey::ShortcutUndo => Some("Strg+Z"),
        TextKey::HelpRedo => Some("Wiederholen"),
        TextKey::ShortcutRedo => Some("Ctrl+Y"),
        TextKey::HairPresetSection => Some("Haar-Voreinstellungen"),
        TextKey::HairPresetLoad => Some("Laden"),
        TextKey::HairToolPick => Some("Ebene waehlen"),
        TextKey::HairToolPickHint => Some(
            "Auf eine Straehne im Viewport klicken, um ihre Ebene zu aktivieren. Der Pinsel wartet auf den naechsten Klick.",
        ),
        TextKey::HelpHairPick => Some("Ebene unter dem Zeiger waehlen"),
        TextKey::ShortcutHairPick => Some("V, oder Strg + Klick mit jedem Pinsel"),
        TextKey::HelpHairSmooth => Some("Glaetten statt kaemmen"),
        TextKey::ShortcutHairSmooth => Some("Shift beim Kaemmen"),
        TextKey::HairGroupScalp => Some("Kopfhaut"),
        TextKey::HairScalpAlpha => Some("Kopfhaut-Maske"),
        TextKey::HairScalpPanel => Some("Kopfhaut"),
        TextKey::HairScalpMesh => Some("Kopfhaut-Mesh"),
        TextKey::HairScalpCreate => Some("Kopfhaut erstellen"),
        TextKey::HairScalpAbsent => Some("Diese Frisur hat noch keine Kopfhaut-Ebene."),
        TextKey::HistoryBranchTitle => Some("Die Schritte danach gehen verloren"),
        TextKey::HistoryBranchProceed => Some("Trotzdem bearbeiten"),
        TextKey::DoNotShowAgain => Some("Nicht mehr anzeigen"),
        TextKey::SurfaceBaseUnblended => {
            Some("Einige PBR-Maps konnten nicht mit der VaM-Haut verschmolzen werden")
        }
        TextKey::DetailEditRedone => Some("Rückgängig gemachte Bearbeitung wiederhergestellt"),
        TextKey::ShortcutBrushSmaller => Some("Pinsel verkleinern"),
        TextKey::ShortcutBrushLarger => Some("Pinsel vergrößern"),
        TextKey::ShortcutBrushSizeDrag => Some("Pinselgröße, gezogen"),
        TextKey::ShortcutBrushStrengthDrag => Some("Pinselstärke, gezogen"),
        TextKey::ShortcutStencilCancel => Some("Schablonenplatzierung abbrechen"),
        TextKey::ShortcutViewOrbit => Some("Ansicht umkreisen"),
        TextKey::ShortcutViewPan => Some("Ansicht verschieben"),
        TextKey::ShortcutViewDolly => Some("Ansicht heran- und wegfahren"),
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
        TextKey::SculptBrushMask => Some("Maske"),
        TextKey::SculptBrushMaskTooltip => Some(
            "Traegt die gewaehlte Aussehens-Ebene ab, sodass die darunter erscheint. Alt malt zurueck",
        ),
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

pub(super) fn hair_label(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some("Haarmenge"),
        "curveDensity" => Some("Kurvendichte"),
        "iterations" => Some("Iterationen"),
        "simulationEnabled" => Some("Physik"),
        "collisionEnabled" => Some("Kollision"),
        "weight" => Some("Gewicht"),
        "drag" => Some("Luftwiderstand"),
        "gravityMultiplier" => Some("Schwerkraft-Faktor"),
        "collisionRadius" => Some("Kollisionsradius"),
        "collisionRadiusRoot" => Some("Kollisionsradius (Ansatz)"),
        "friction" => Some("Reibung"),
        "bendResistance" => Some("Biegewiderstand"),
        "rootRigidity" => Some("Steifigkeit am Ansatz"),
        "mainRigidity" => Some("Steifigkeit in der Mitte"),
        "tipRigidity" => Some("Steifigkeit an der Spitze"),
        "jointRigidity" => Some("Gelenksteifigkeit"),
        "rigidityRolloffPower" => Some("Steifigkeitsabfall"),
        "cling" => Some("Stil-Zusammenhalt"),
        "clingRolloff" => Some("Zusammenhalts-Abfall"),
        "snap" => Some("Einrasten"),
        "usePaintedRigidity" => Some("Gemalte Steifigkeit nutzen"),
        "width" => Some("Dicke"),
        "length1" => Some("Länge 1"),
        "length2" => Some("Länge 2"),
        "length3" => Some("Länge 3"),
        "maxSpread" => Some("Maximale Streuung"),
        "spreadRoot" => Some("Streuung (Ansatz)"),
        "spreadMid" => Some("Streuung (Mitte)"),
        "spreadTip" => Some("Streuung (Spitze)"),
        "spreadMidpoint" => Some("Streuungs-Mittelpunkt"),
        "spreadCurvePower" => Some("Streuungskurve"),
        "curlScale" => Some("Lockengröße"),
        "curlFrequency" => Some("Lockenfrequenz"),
        "curlScaleRandomness" => Some("Größen-Zufall"),
        "curlFrequencyRandomness" => Some("Frequenz-Zufall"),
        "curlRoot" => Some("Locke (Ansatz)"),
        "curlMid" => Some("Locke (Mitte)"),
        "curlTip" => Some("Locke (Spitze)"),
        "curlMidpoint" => Some("Locken-Mittelpunkt"),
        "curlCurvePower" => Some("Lockenkurve"),
        "curlX" => Some("Locke X"),
        "curlY" => Some("Locke Y"),
        "curlZ" => Some("Locke Z"),
        "curlNormalAdjust" => Some("Locken-Normalenkorrektur"),
        "curlAllowReverse" => Some("Gegenrichtung erlauben"),
        "curlAllowFlipAxis" => Some("Achsenspiegelung erlauben"),
        "Alpha Adjust" => Some("Kopfhaut-Deckkraft"),
        "Diffuse Color" => Some("Kopfhaut-Diffusfarbe"),
        "Gloss" => Some("Kopfhaut-Glanz"),
        "Specular Intensity" => Some("Kopfhaut-Spiegelung"),
        "Diffuse Texture Offset" => Some("Kopfhaut-Texturversatz"),
        "rootColor" => Some("Ansatzfarbe"),
        "tipColor" => Some("Spitzenfarbe"),
        "specularColor" => Some("Glanzfarbe"),
        "colorRolloff" => Some("Farbabfall Ansatz→Spitze"),
        "randomColorPower" => Some("Zufallsfarben-Stärke"),
        "randomColorOffset" => Some("Zufallsfarben-Versatz"),
        "primarySpecularSharpness" => Some("Primärer Glanz"),
        "secondarySpecularSharpness" => Some("Sekundärer Glanz"),
        "specularShift" => Some("Versatz des zweiten Glanzes"),
        "diffuseSoftness" => Some("Diffus-Weichheit"),
        "fresnelPower" => Some("Fresnel-Stärke"),
        "fresnelAttenuation" => Some("Fresnel-Abschwächung"),
        "IBLFactor" => Some("Indirektes Licht"),
        "normalRandomize" => Some("Normalen-Zufall"),
        _ => None,
    }
}

pub(super) fn hair_hint(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some(
            "Wie viele Strähnen zwischen je drei Führungen wachsen. Das Aussehen des Haares, und der Großteil seiner Kosten.",
        ),
        "curveDensity" => Some(
            "Punkte entlang jeder Strähne. Mehr ist glatter und langsamer; die Form ist lange lesbar, bevor die Zahl hoch wird.",
        ),
        "iterations" => Some(
            "Solver-Durchgänge pro Bild. Mehr hält die Zwangsbedingungen bei schneller Bewegung straffer.",
        ),
        "simulationEnabled" => Some(
            "Ob sich dieses Part im Spiel bewegt. Hat nichts mit dem Viewport-Schalter zu tun.",
        ),
        "collisionEnabled" => Some("Ob die Strähnen dieses Parts aus dem Körper geschoben werden."),
        "weight" => Some(
            "Wie schwer eine Strähne hängt. Skaliert Schwerkraft und den Widerstand gegen Bewegung.",
        ),
        "drag" => Some(
            "Wie stark die Luft eine Strähne bremst. Hoch legt sie sich schnell und schwingt kaum.",
        ),
        "gravityMultiplier" => Some("Die Schwerkraft auf dieses Part, als Vielfaches der Szene."),
        "collisionRadius" => {
            Some("Die Dicke, mit der eine Strähne kollidiert, über ihre ganze Länge.")
        }
        "collisionRadiusRoot" => {
            Some("Die Dicke nahe dem Ansatz, wenn sie vom Rest abweichen soll.")
        }
        "friction" => Some("Wie sehr eine Strähne greift statt zu gleiten."),
        "bendResistance" => {
            Some("Wie sehr eine Strähne sich dem Biegen aus der Geraden widersetzt.")
        }
        "rootRigidity" => Some("Wie stark der Ansatz die frisierte Form hält."),
        "mainRigidity" => Some("Wie stark die Mitte die frisierte Form hält."),
        "tipRigidity" => Some("Wie stark die Spitze die frisierte Form hält."),
        "jointRigidity" => Some(
            "Die Härte der Stil-Gelenke: die Federn, die einen Pferdeschwanz zum Pferdeschwanz machen.",
        ),
        "rigidityRolloffPower" => Some(
            "Wie sich die drei Steifigkeiten entlang der Strähne mischen. Höher trägt den Ansatzwert weiter nach unten.",
        ),
        "cling" => Some("Wie stark die Stil-Gelenke Strähnen zueinander ziehen."),
        "clingRolloff" => Some(
            "Wie weit der Zusammenhalt entlang der Strähne abklingt. Höher hält ihn nahe am Ansatz.",
        ),
        "snap" => Some("Wie fest jedes Segment die erstellte Länge hält."),
        "usePaintedRigidity" => {
            Some("Die Steifigkeit aus der gemalten Karte nehmen statt aus den drei Reglern.")
        }
        "width" => Some(
            "Wie dick eine Strähne gezeichnet wird. Auch der Regler in VaM endet bei einem Zehntelmillimeter.",
        ),
        "length1" => Some("Länge der Strähnen nahe der ersten Führung ihres Dreiecks."),
        "length2" => Some("Länge der Strähnen nahe der zweiten Führung ihres Dreiecks."),
        "length3" => Some("Länge der Strähnen nahe der dritten Führung ihres Dreiecks."),
        "maxSpread" => Some(
            "Eine Obergrenze dafür, wie weit eine Strähne von ihrer Führung abdriften darf, kein Verstärker. Sobald sie den Führungsabstand übersteigt, ändert Höherdrehen nichts.",
        ),
        "spreadRoot" => {
            Some("Wie frei sich Strähnen am Ansatz trennen. Niedrig sammelt sie zur Führung hin.")
        }
        "spreadMid" => Some("Wie frei sich Strähnen in der Mitte trennen."),
        "spreadTip" => Some("Wie frei sich Strähnen an der Spitze trennen."),
        "spreadMidpoint" => Some("Ab welcher Stelle der Strähne die mittlere Streuung greift."),
        "spreadCurvePower" => {
            Some("Wie scharf die Streuung zwischen Ansatz, Mitte und Spitze umschlägt.")
        }
        "curlScale" => {
            Some("Die Breite der Locke: der Radius, um den sie sich windet, nicht ihre Länge.")
        }
        "curlFrequency" => Some("Windungen pro Zentimeter Strähne."),
        "curlScaleRandomness" => {
            Some("Wie stark die Lockenbreite von Strähne zu Strähne schwankt.")
        }
        "curlFrequencyRandomness" => {
            Some("Wie stark die Windungsrate von Strähne zu Strähne schwankt.")
        }
        "curlRoot" => Some("Wie viel Locke den Ansatz erreicht."),
        "curlMid" => Some("Wie viel Locke die Mitte erreicht."),
        "curlTip" => Some("Wie viel Locke die Spitze erreicht."),
        "curlMidpoint" => Some("Ab welcher Stelle der Strähne die mittlere Locke greift."),
        "curlCurvePower" => {
            Some("Wie scharf die Locke zwischen Ansatz, Mitte und Spitze umschlägt.")
        }
        "curlX" => Some(
            "Die Verschiebung der Locke entlang X. Ein Vektor, der gedreht wird, keine Achse zum Winden.",
        ),
        "curlY" => Some("Die Verschiebung der Locke entlang Y."),
        "curlZ" => Some("Die Verschiebung der Locke entlang Z."),
        "curlNormalAdjust" => Some(
            "Wird dem Lockenversatz entlang der Normalen zugerechnet. Es hebt die Windung vom Kopf ab, statt sie zu neigen.",
        ),
        "curlAllowReverse" => Some(
            "Lässt manche Strähnen andersherum winden, damit ein Lockenkopf nicht als eine wiederholte Strähne liest.",
        ),
        "curlAllowFlipAxis" => {
            Some("Lässt manche Strähnen um eine gespiegelte Achse winden, aus demselben Grund.")
        }
        "Alpha Adjust" => Some(
            "Wie deckend die Kopfhaut-Kappe ist. Die meisten Parts tragen die Kappe eines anderen und lassen dies ganz unten, um sie zu verbergen; erhöhe es nur, wenn ein Scheitel Kopfhaut zeigen soll.",
        ),
        "Diffuse Color" => Some(
            "Die Diffusfarbe der Kappe selbst. VaM liefert sie weiß, damit die Haut darunter durchscheint.",
        ),
        "Gloss" => Some("Wie glänzend die Kappe dort ist, wo sie sichtbar wird."),
        "Specular Intensity" => Some("Wie stark die Kappe ein Glanzlicht aufnimmt."),
        "Diffuse Texture Offset" => {
            Some("Wird der Diffusfarbe der Kappe zugerechnet. Trotz des Namens kein Blattindex.")
        }
        "rootColor" => Some("Die Farbe am Ansatz."),
        "tipColor" => Some("Die Farbe an der Spitze."),
        "specularColor" => Some("Die Farbe des Glanzlichts."),
        "colorRolloff" => Some("Wie die Ansatzfarbe entlang der Strähne der Spitzenfarbe weicht."),
        "randomColorPower" => Some("Wie stark Strähnen in der Farbe voneinander abweichen."),
        "randomColorOffset" => Some("Wohin diese Abweichung neigt, heller oder dunkler."),
        "primarySpecularSharpness" => {
            Some("Wie eng das Hauptglanzlicht ist. Höher ist glänzender.")
        }
        "secondarySpecularSharpness" => {
            Some("Wie eng das zweite Glanzlicht ist: das farbige, das mit dem Haar wandert.")
        }
        "specularShift" => {
            Some("Wie weit das zweite Glanzlicht entlang der Strähne vom ersten sitzt.")
        }
        "diffuseSoftness" => Some(
            "Wie weich Licht sich um eine Strähne legt, statt nur die zugewandte Seite zu beleuchten.",
        ),
        "fresnelPower" => {
            Some("Wie scharf der Rand aufhellt, wo sich das Haar vom Blick wegdreht.")
        }
        "fresnelAttenuation" => Some("Wie stark diese Randaufhellung ist."),
        "IBLFactor" => Some("Wie viel Umgebungslicht der Szene das Haar aufnimmt."),
        "normalRandomize" => Some(
            "Wie stark die Schattierungsnormale jeder Strähne verwackelt wird, damit Nachbarn nicht identisch schattieren.",
        ),
        _ => None,
    }
}
