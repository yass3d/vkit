use super::TextKey;

pub(super) const fn label(key: TextKey) -> Option<&'static str> {
    #[allow(unreachable_patterns, reason = "the fallback outlives today's key set")]
    match key {
        TextKey::SurfaceSmooth => Some("Lissage du maillage"),
        TextKey::SurfaceSmoothTooltip => Some(
            "Lissage de rendu compatible VaM. 0 correspond au maillage de base éditable, 3 à la valeur par défaut de VaM et 4 au maximum. Les morphs, les UV, la sculpture et le bake de textures conservent la topologie de base",
        ),
        TextKey::FaceMatch => Some("Créer"),
        TextKey::ResultPreview => Some("Enregistrer"),
        TextKey::Save => Some("Enregistrer"),
        TextKey::DetailCorrection => Some("Sculpter"),
        TextKey::TextureStage => Some("Texture"),
        TextKey::HairStage => Some("Cheveux"),
        TextKey::HairUnavailable => {
            Some("Terminez le résultat de sculpture avant de modifier les cheveux")
        }
        TextKey::HairPartsPanel => Some("Parties de cheveux"),
        TextKey::AddHairPart => Some("Ajouter une partie de cheveux"),
        TextKey::HairScalpMeshCrowded => Some(
            "Certaines mèches n'avaient pas de place sur le nouveau maillage de cuir chevelu et ont été supprimées",
        ),
        TextKey::HairScalpMissing => {
            Some("Maillage du cuir chevelu introuvable ; ouvrez une racine VaM et rescannez")
        }
        TextKey::HairShowPoints => Some("Afficher les points"),
        TextKey::HairShowPointsHint => Some(
            "Affiche les points de plantation du cuir chevelu. Desactivez pour ne voir que les cheveux ; les pinceaux fonctionnent toujours.",
        ),
        TextKey::HairPartTint => Some("Teinter les parties"),
        TextKey::HairPartTintHint => Some(
            "Donne à chaque partie une teinte distincte dans la vue pour les distinguer. Affichage seul ; l export n est pas touché.",
        ),
        TextKey::HairSettingsPanel => Some("Reglages cheveux"),
        TextKey::HairCreateFirst => Some("Creez des cheveux d'abord."),
        TextKey::HairHideStrands => Some("Masquer les cheveux"),
        TextKey::HairHideStrandsHint => Some(
            "Retire les cheveux generes pour ne lire que les meches guides. L'activer active aussi les meches.",
        ),
        TextKey::HairShowStreams => Some("Voir les meches guides"),
        TextKey::HairShowStreamsHint => {
            Some("Dessine la meche que vous editez, par-dessus les cheveux qu'elle produit.")
        }
        TextKey::HairViewportPhysics => Some("Physique du viewport"),
        TextKey::HairViewportPhysicsHint => {
            Some("Previsualise la physique a l'ecran. Sans rapport avec l'export.")
        }
        TextKey::HairMirrorEdit => Some("Édition miroir"),
        TextKey::HairMirrorEditHint => {
            Some("Les brosses éditent les deux côtés à la fois, en symétrie gauche-droite.")
        }
        TextKey::HairAutoPart => Some("Partie auto"),
        TextKey::HairAutoPartHint => Some(
            "La brosse n'édite que la partie reconnue sous le curseur, au lieu des calques actifs.",
        ),
        TextKey::HairExportSection => Some("Enregistrer les cheveux"),
        TextKey::HairExportBothSexes => Some("Les deux"),
        TextKey::HairExportRescanNotice => {
            Some("Cliquez VaM - Hair - Rescan Files pour le voir dans la liste")
        }
        TextKey::HairExportOverwroteNotice => {
            Some("Une vignette remplacée apparaît après Hard Reset ou un redémarrage de VaM")
        }
        TextKey::HairSimToggleHint => Some("Execute la physique des cheveux ici et dans le jeu."),
        TextKey::HairCollisionToggleHint => {
            Some("Empeche les cheveux de traverser la tete et le corps.")
        }
        TextKey::HairStyleJoints => Some("Joints de style"),
        TextKey::HairStyleJointsHint => Some(
            "Relie les mèches proches pour que la coiffure garde sa forme en jeu ; c est ce que Cling module. Pour cheveux longs ou attachés ; alourdit le fichier.",
        ),
        TextKey::HairThumbnailPrompt => Some("Cadrez votre angle puis cliquez pour capturer"),
        TextKey::HairThumbnailShoot => Some("Capturer"),
        TextKey::HairThumbnailSkip => Some("Passer"),
        TextKey::HairThumbnailSaved => Some("Vignette enregistrée"),
        TextKey::HairThumbnailFailed => Some("Échec de la vignette"),
        TextKey::HairGameOnly => Some(
            "Réglage physique : la fenêtre ne simule pas, il n'agit donc que dans VaM. Exporté avec le style.",
        ),
        TextKey::HairPartVisible => Some("Afficher ou masquer cette partie de cheveux"),
        TextKey::RemoveHairPart => Some("Supprimer la partie de cheveux"),
        TextKey::HairRenamePart => Some("Cliquer pour renommer"),
        TextKey::HairToolPlant => Some("Planter"),
        TextKey::HairToolErase => Some("Effacer"),
        TextKey::HairToolGrow => Some("Allonger (Alt raccourcit)"),
        TextKey::HairToolComb => Some("Peigner"),
        TextKey::HairToolPlantHint => {
            Some("Plante des meches sur les sommets du cuir chevelu sous la brosse")
        }
        TextKey::HairToolGrowHint => {
            Some("Allonge les meches sous la brosse; Alt pour les raccourcir")
        }
        TextKey::HairToolEraseHint => Some("Efface les meches sous la brosse"),
        TextKey::HairToolCombHint => {
            Some("Coiffe les meches sous la brosse; les racines restent en place")
        }
        TextKey::HairToolPinch => Some("Resserrer"),
        TextKey::HairToolPinchHint => Some("Rassemble les meches sous le pinceau. Alt les ecarte"),
        TextKey::HairToolCut => Some("Couper"),
        TextKey::HairToolCutHint => Some("Coupe les meches la ou le pinceau les traverse"),
        TextKey::HairToolPuff => Some("Gonfler"),
        TextKey::HairToolPuffHint => Some("Redresse les meches sous le pinceau. Alt les aplatit"),
        TextKey::HairToolVertex => Some("Point"),
        TextKey::HairToolVertexHint => Some(
            "Saisissez une articulation d'une mèche et déplacez-la ; la longueur est conservée",
        ),
        TextKey::HairCopySettings => Some("Tout copier"),
        TextKey::HairPasteSettings => Some("Tout coller"),
        TextKey::HairGroupPerformance => Some("Performance"),
        TextKey::HairGroupPhysics => Some("Physique"),
        TextKey::HairGroupStiffness => Some("Rigidite"),
        TextKey::HairGroupShape => Some("Forme"),
        TextKey::HairGroupCurl => Some("Boucle"),
        TextKey::HairGroupLook => Some("Aspect"),
        TextKey::HairColorScalp => Some("Cuir"),
        TextKey::HairColorRoot => Some("Racine"),
        TextKey::HairColorTip => Some("Pointe"),
        TextKey::HairColorSpecular => Some("Eclat"),
        TextKey::HairScalpMask => Some("Texture du cuir"),
        TextKey::HairScalpMaskBuiltIn => Some("Integre"),
        TextKey::HairScalpMaskCustom => Some("Depuis un fichier..."),
        TextKey::HairExport => Some("Exporter vers VaM"),
        TextKey::HairExportName => Some("Nom de l objet"),
        TextKey::HairExportCreator => Some("Createur"),
        TextKey::HairExportNeedsMetadata => Some("Indiquez le nom et le createur avant d exporter"),
        TextKey::HairOverwriteTitle => Some("Remplacer ce style ?"),
        TextKey::HairOverwriteBody => Some(
            "Un style de ce nom et de ce createur existe deja. L export remplace ses fichiers.",
        ),
        TextKey::HairOverwriteProceed => Some("Remplacer"),
        TextKey::HairExportDone => Some("Objet cheveux enregistre"),
        TextKey::HairExportFailed => Some("Echec de l export des cheveux"),
        TextKey::HairExportNeedsPart => Some("Selectionnez d abord une partie avec des meches"),
        TextKey::HairExportNeedsVaMRoot => Some("Definissez une racine VaM avant d exporter"),
        TextKey::HairMirrorPart => Some("Refleter la partie de l autre cote"),
        TextKey::HairDuplicatePart => Some("Dupliquer la partie"),
        TextKey::HairSegmentsShort => Some("Seg"),
        TextKey::MorphCategoryBody => Some("Corps"),
        TextKey::TextureNamePlaceholder => Some("Saisissez un nom de texture"),
        TextKey::TextureOverwriteTitle => Some("Cela remplace les textures du même nom"),
        TextKey::OpenScan => Some("Scan"),
        TextKey::SourceMorphMissing => Some("Aucun morph de ce look n'est installé"),
        TextKey::EditDetails => Some("Éditer les détails"),
        TextKey::Female => Some("Femme"),
        TextKey::Male => Some("Homme"),
        TextKey::Lighting => Some("Éclairage"),
        TextKey::Brightness => Some("Luminosité"),
        TextKey::LightRotation => Some("Rotation lumière"),
        TextKey::AddHeadFileHint => Some("ou glissez ici un fichier OBJ, GLB ou FBX"),
        TextKey::AddHeadFile => Some("Ajouter un fichier de tête"),
        TextKey::AutoAlign => Some("Alignement auto"),
        TextKey::PlacementReset => Some("Réinit. placement"),
        TextKey::Scale => Some("Échelle"),
        TextKey::Position => Some("Position"),
        TextKey::Rotation => Some("Rotation"),
        TextKey::XMirror => Some("Miroir X"),
        TextKey::XMirrorTooltip => Some("Poser un repère pose aussi son jumeau de l'autre côté"),
        TextKey::NumbersTooltip => Some("Numérote chaque repère dans son ordre de placement"),
        TextKey::XMirrorRejected => Some("Le miroir X a été désactivé faute de contrepartie sûre"),
        TextKey::ScanFidelity => Some("Fidélité au scan"),
        TextKey::ScanOptions => Some("Options du scan"),
        TextKey::RestoreNeckEars => Some("Restaurer cou et oreilles"),
        TextKey::RestoreNeckEarsTooltip => Some(
            "Garde oreilles, nuque et arrière du crâne sur la base G2, en fondant l'ajustement seulement vers le visage",
        ),
        TextKey::RestoreNeckEarsDeferred => Some(
            "Des coups de sculpture existent ; le changement s'appliquera à la prochaine génération",
        ),
        TextKey::ScanFidelityTooltip => Some(
            "Au-delà de 1, l'ajustement suit davantage la forme du scan et conserve moins la douceur de la figure. Jusqu'à 2, c'est tout ce qu'il fait ; au-delà de 2, le bruit du scan — grains, trous, coutures de raccord — ressort aussi sous forme de bosses, ce qui peut valoir le coup sur un scan propre et dense. Les repères ne sont jamais sacrifiés, quel que soit le réglage, et aucune protection n'est relâchée.",
        ),
        TextKey::ScanSymmetry => Some("Symétrie du scan"),
        TextKey::Original => Some("Original"),
        TextKey::Left => Some("Gauche"),
        TextKey::Right => Some("Droite"),
        TextKey::EyesOpen => Some("Yeux ouverts"),
        TextKey::EyesClosed => Some("Yeux fermés"),
        TextKey::View => Some("Options de vue"),
        TextKey::On => Some("Activé"),
        TextKey::Numbers => Some("Numéros"),
        TextKey::PinPairsPlaced => Some("Repères"),
        TextKey::PinPlacement => Some("Placement des repères"),
        TextKey::ScanReference => Some("Référence du scan"),
        TextKey::AddHeadFileAction => Some("Ajouter un fichier de tête"),
        TextKey::AddHeadFileActionTooltip => {
            Some("Charge un fichier de tête scannée (OBJ · GLB · FBX)")
        }
        TextKey::ContinueStep => Some("Continuer avec la tête de base"),
        TextKey::ContinueStepTooltip => Some(
            "Modifie la tête G2 telle quelle, sans scan ; un scan pourra être chargé et ajusté plus tard",
        ),
        TextKey::ResetAllPins => Some("Réinitialiser les repères"),
        TextKey::ResetAll => Some("Tout reinitialiser"),
        TextKey::MorphSave => Some("Enregistrer le morph"),
        TextKey::TextureSaveSection => Some("Enregistrer les textures"),
        TextKey::Generate => Some("Ajuster le visage"),
        TextKey::Cancel => Some("Annuler"),
        TextKey::CopyAll => Some("Tout copier"),
        TextKey::Next => Some("Enregistrer"),
        TextKey::TextureSkin => Some("Préréglage"),
        TextKey::SolidColor => Some("Uni"),
        TextKey::BackfaceProtectionTooltip => Some("Protège les faces arrière des maillages fins"),
        TextKey::WireframeColor => Some("Couleur du filaire"),
        TextKey::WireframeSettings => Some("Filaire"),
        TextKey::XraySettings => Some("Rayons X"),
        TextKey::ViewportLightingTooltip => Some("Régler l'éclairage"),
        TextKey::Background => Some("Arrière-plan"),
        TextKey::ViewportBackgroundTooltip => Some("Régler l'arrière-plan et l'image de référence"),
        TextKey::BackgroundRadial => Some("Radial"),
        TextKey::BackgroundVertical => Some("Vertical"),
        TextKey::BackgroundFlat => Some("Uni"),
        TextKey::ViewportWireframeTooltip => Some("Afficher et régler la surimpression filaire"),
        TextKey::ViewportXrayTooltip => Some("Afficher et régler la surimpression rayons X"),
        TextKey::ViewportSkinTooltip => Some("Choisir une texture de peau VaM"),
        TextKey::FalloffTooltip => Some("Choisir la courbe d'influence de la brosse"),
        TextKey::FalloffSmooth => Some("Douce"),
        TextKey::FalloffSmoother => Some("Plus douce"),
        TextKey::FalloffSharp => Some("Nette"),
        TextKey::FalloffLinear => Some("Linéaire"),
        TextKey::PlaceHead => Some("Placer la tête"),
        TextKey::PlaceHeadTooltip => Some(
            "Superpose la tête chargée sur G2 pour aligner sa position, sa rotation et sa taille.",
        ),
        TextKey::ScanOverlay => Some("Surimpression du scan"),
        TextKey::OverlayDone => Some("Terminer la surimpression"),
        TextKey::ProjectImage => Some("Projeter"),
        TextKey::ProjectionCanvasHint => {
            Some("Peignez dans la vue 3D et le trait arrive sur ce canevas UV")
        }
        TextKey::HelpProjectionPaint => Some("Peindre (projeter)"),
        TextKey::ShortcutProjectionPaint => Some("Glisser gauche"),
        TextKey::HelpProjectionMove => Some("Déplacer l'image"),
        TextKey::ShortcutProjectionMove => Some("Glisser droit"),
        TextKey::HelpProjectionRotate => Some("Pivoter l'image"),
        TextKey::ShortcutProjectionRotate => Some("Maj + glisser droit"),
        TextKey::HelpProjectionZoom => Some("Redimensionner l'image"),
        TextKey::ShortcutProjectionZoom => Some("Molette"),
        TextKey::HelpBrushSize => Some("Taille de la brosse"),
        TextKey::ShortcutBrushSize => Some("Glisser F · [ ]"),
        TextKey::HelpBrushStrength => Some("Intensité du pinceau"),
        TextKey::ShortcutBrushStrength => Some("Shift+F glisser"),
        TextKey::HelpProjectionDone => Some("Terminer la projection"),
        TextKey::ShortcutProjectionDone => Some("Terminer · Échap"),
        TextKey::ProjectDone => Some("Terminer"),
        TextKey::ProjectDoneTooltip => {
            Some("Termine la projection ; tout ce qui est peint reste sur le calque")
        }
        TextKey::MorphCompare => Some("Comparer"),
        TextKey::Opacity => Some("Opacité"),
        TextKey::G2Head => Some("Tête G2"),
        TextKey::Eyelashes => Some("Cils"),
        TextKey::OverwriteTitle => Some("Écraser le fichier"),
        TextKey::OverwriteBody => Some("Un fichier du même nom existe déjà. L'écraser ?"),
        TextKey::Overwrite => Some("Écraser"),
        TextKey::ScanTextureLayer => Some("Texture du scan"),
        TextKey::PinPrompt => Some("Placez un repère sur chacun, ou ajustez sans"),
        TextKey::TextureNeedsImage => Some("Ajoutez une image depuis le panneau latéral"),
        TextKey::TexturePinPairPrompt => {
            Some("Placez des broches correspondantes à gauche et à droite")
        }
        TextKey::TextureToolNeedsPins => Some("Placez d'abord les repères pour l'utiliser"),
        TextKey::SymmetrySuggestion => Some("Le miroir est conseillé pour un ajustement plus net"),
        TextKey::SymmetryChangeTitle => Some("Les repères seront réinitialisés"),
        TextKey::SymmetryChangeConfirm => Some("Confirmer"),
        TextKey::EyeClosure => Some("Fermer les yeux"),
        TextKey::VaMFolder => Some("Dossier VaM"),
        #[cfg(test)]
        TextKey::VaMGeometryBaseReady => Some("Base de géométrie VaM vérifiée"),
        TextKey::VaMGeometryBaseRejected => Some("Base de géométrie VaM inutilisable"),
        TextKey::VaMGeometryBaseHowTo => Some(
            "Sélectionnez votre dossier VaM ; la base neutre G2 est extraite automatiquement de votre installation",
        ),
        TextKey::VaMBaseSourceUnityBundle => {
            Some("Base neutre intégrée de VaM (extraite automatiquement)")
        }
        TextKey::VaMBaseSourceDazAnchorPair => Some("Ancre DAZ + base OBJ vérifiée"),
        TextKey::VaMBaseSourceUnityAnchorPair => Some("Ancre neutre extraite + base OBJ vérifiée"),
        TextKey::VaMBaseSourceUnverifiedObj => Some("Base OBJ non vérifiée"),
        TextKey::VaMUvMappingUnavailable => Some(
            "Le mappage UV de la peau VaM n'est pas prêt. Vérifiez votre dossier VaM et actualisez le catalogue de peaux",
        ),
        TextKey::VaMIndexing => Some("Indexation du catalogue VaM"),
        TextKey::VaMFailed => Some("Échec du catalogue VaM"),
        TextKey::RefreshSkins => Some("Actualiser la bibliothèque de peaux"),
        TextKey::MorphLoading => Some("Chargement du morph"),
        TextKey::MorphLoadFailed => Some("Échec du chargement du morph"),
        TextKey::DefaultSkinMissing => Some("Peau introuvable. Vérifiez le dossier VaM"),
        TextKey::DefaultSkinSet => Some("Ouvrir avec cette peau"),
        TextKey::DefaultSkinClear => Some("Ne plus ouvrir avec cette peau"),
        TextKey::SkinNone => Some("Aucune"),
        TextKey::SkinSearch => Some("Rechercher des peaux"),
        TextKey::SkinLoading => Some("Chargement de la peau"),
        TextKey::SkinReady => Some("Aperçu de la peau prêt"),
        TextKey::SkinLoadFailed => Some("Échec du chargement de la peau"),
        TextKey::SkinUvUnavailable => Some("UV de peau indisponible"),
        TextKey::NoMatchingSkins => Some("Aucune peau correspondante"),
        TextKey::HairSearch => Some("Rechercher des cheveux"),
        TextKey::HairReady => Some("Aperçu des cheveux prêt"),
        TextKey::HairLoadFailed => Some("Échec du chargement des cheveux"),
        TextKey::HairPartsSkipped => Some("Affiché sans certaines parties"),
        TextKey::HairParts => Some("parties"),
        TextKey::BaseFace => Some("Visage de base"),
        TextKey::Settings => Some("Paramètres"),
        TextKey::SettingsGraphics => Some("Graphismes"),
        TextKey::SettingsViewport => Some("Vue 3D"),
        TextKey::SettingsGeneral => Some("Général"),
        TextKey::SettingsAbout => Some("À propos"),
        TextKey::SettingsShortcuts => Some("Raccourcis"),
        TextKey::ShortcutsCapturing => Some("Appuyez…"),
        TextKey::ShortcutsResetAll => Some("Réinitialiser tous les raccourcis"),
        TextKey::ShortcutGroupSystem => Some("Partout"),
        TextKey::ShortcutGroupAlignment => Some("Placer la tête"),
        TextKey::ShortcutGroupSculpt => Some("Pinceaux de sculpture"),
        TextKey::ShortcutGroupHair => Some("Pinceaux de cheveux"),
        TextKey::ShortcutGroupView => Some("Vues de caméra"),
        TextKey::ShortcutGroupNavigation => Some("Changer d'onglet"),
        TextKey::ShortcutViewReset => Some("Réinitialiser la caméra"),
        TextKey::ShortcutViewProjection => Some("Perspective / orthographique"),
        TextKey::ShortcutViewFront => Some("Vue de face"),
        TextKey::ShortcutViewLeftSide => Some("Vue de gauche"),
        TextKey::ShortcutViewRightSide => Some("Vue de droite"),
        TextKey::ShortcutViewTop => Some("Vue de dessus"),
        TextKey::ShortcutViewBottom => Some("Vue de dessous"),
        TextKey::ShortcutViewFrontUpperLeft => Some("Face haut gauche"),
        TextKey::ShortcutViewFrontUpperRight => Some("Face haut droite"),
        TextKey::ShortcutViewFrontLowerLeft => Some("Face bas gauche"),
        TextKey::ShortcutViewFrontLowerRight => Some("Face bas droite"),
        TextKey::ShortcutTabFaceMatch => Some("Onglet ajustement du visage"),
        TextKey::ShortcutTabDetail => Some("Onglet détail"),
        TextKey::ShortcutTabTexture => Some("Onglet texture"),
        TextKey::ShortcutTabHair => Some("Onglet cheveux"),
        TextKey::ShortcutTabSave => Some("Onglet enregistrement"),
        TextKey::ShortcutGroupLists => Some("Calques et parties"),
        TextKey::ShortcutGroupTexture => Some("Toile de texture"),
        TextKey::ReferenceImages => Some("Référence"),
        TextKey::ReferenceImagesEmpty => Some("Aucune image de référence"),
        TextKey::ReferenceImageAdd => Some("Ajouter une image"),
        TextKey::ReferenceImageRemove => Some("Retirer"),
        TextKey::ReferenceImageRaise => Some("Mettre au premier plan"),
        TextKey::ReferenceImageLower => Some("Mettre en arrière"),
        TextKey::ShortcutTextureCanvasPan => Some("Déplacer la toile (maintenir)"),
        TextKey::ShortcutDiagnosticLogCopy => Some("Copier le journal"),
        TextKey::ShortcutLightRotate => Some("Tourner la lumière (maintenir)"),
        TextKey::ShortcutLayerHide => Some("Masquer le calque"),
        TextKey::ShortcutLayerUnhideAll => Some("Afficher tous les calques"),
        TextKey::ShortcutLayerIsolate => Some("Masquer les autres calques"),
        TextKey::ShortcutLayerLocalView => Some("Afficher seulement celui-ci (bascule)"),
        TextKey::ShortcutLayerInvertSelection => Some("Inverser la sélection"),
        TextKey::ShortcutSculptSmoothHold => Some("Maintenir pour lisser"),
        TextKey::ShortcutSculptInflateHold => Some("Maintenir pour gonfler"),
        TextKey::ShortcutSculptAlternateHold => Some("Maintenir pour inverser"),
        TextKey::ShortcutHairSmoothHold => Some("Maintenir pour lisser les cheveux"),
        TextKey::ShortcutHairInvertHold => Some("Maintenir pour inverser l'outil cheveux"),
        TextKey::ShortcutTextureInvertHold => Some("Maintenir pour inverser l'outil texture"),
        TextKey::ShortcutListAddToSelection => Some("Maintenir pour ajouter à la sélection"),
        TextKey::ShortcutListSolo => Some("Maintenir pour isoler"),
        TextKey::ShortcutsExport => Some("Enregistrer le mappage dans un fichier"),
        TextKey::ShortcutsImport => Some("Charger un fichier de mappage"),
        TextKey::ShortcutsTaken => Some("Une autre action y répond déjà"),
        TextKey::SettingsAboutBuild => Some("Build"),
        TextKey::SettingsAboutLicense => Some("Licence"),
        TextKey::SettingsAboutDiagnostics => Some("Diagnostic"),
        TextKey::SettingsTooltipsTooltip => {
            Some("De l'aide comme celle-ci, qui vient d'apparaître")
        }
        TextKey::AboutTagline => Some("Un outil complémentaire pour créer des morphs de tête VaM."),
        TextKey::AboutNoAffiliation => Some(
            "Virt-A-Mate appartient à MeshedVR, Genesis 2 à DAZ 3D. Vkit est un outil indépendant, sans lien avec l'un ni l'autre.",
        ),
        TextKey::AboutNoWarranty => Some(
            "Fourni tel quel, sans garantie. Ce que vous en faites et la licence de ce que vous chargez relèvent de vous.",
        ),
        TextKey::AboutSupport => Some("Soutien"),
        TextKey::AboutReportProblem => Some(
            "Vkit écrit un journal à chaque exécution, et un fichier à part s'il tombe. Ouvrez le dossier et joignez les deux au rapport : une panne que personne d'autre ne voit est une panne que personne ne corrigera.",
        ),
        TextKey::AboutOpenLogs => Some("Ouvrir le dossier des journaux"),
        TextKey::AboutReportIssue => Some("Signaler un problème"),
        TextKey::SettingsLightingPreset => Some("Préréglage"),
        TextKey::SettingsExposure => Some("Exposition"),
        TextKey::SettingsSmoothPasses => Some("Lissage de surface"),
        TextKey::SettingsSmoothPassesHint => Some(
            "Lisse uniquement l'affichage, comme le fait VaM. La forme enregistrée est inchangée.",
        ),
        TextKey::SettingsBackgroundGroup => Some("Arrière-plan"),
        TextKey::SettingsBackground => Some("Style"),
        TextKey::SettingsOverlayGroup => Some("Surimpressions"),
        TextKey::SettingsWireframeColor => Some("Couleur du filaire"),
        TextKey::SettingsWireframeOpacity => Some("Opacité du filaire"),
        TextKey::SettingsXrayOpacity => Some("Opacité des rayons X"),
        TextKey::SettingsSolidColor => Some("Couleur unie de la tête"),
        TextKey::SettingsLanguageGroup => Some("Langue"),
        TextKey::SettingsMorphNames => Some("Noms des morphs intégrés"),
        TextKey::SettingsMorphNamesTranslated => Some("Traduits"),
        TextKey::SettingsMorphNamesOriginal => Some("Original anglais"),
        TextKey::SettingsLanguage => Some("Langue d'affichage"),
        TextKey::SettingsFoldersGroup => Some("Dossiers"),
        TextKey::SettingsVaMRoot => Some("Chemin d'installation de VaM"),
        TextKey::HairNeedsFinishedHead => Some("Les cheveux se portent sur une tête terminée."),
        TextKey::NoMatchingHair => Some("Aucun préréglage de cheveux correspondant"),
        TextKey::MorphSearch => Some("Rechercher des morphs"),
        TextKey::MorphOneSidedFilter => Some("G/D"),
        TextKey::MorphOneSidedFilterShow => Some("Afficher les morphs qui ne bougent qu'un côté"),
        TextKey::MorphOneSidedFilterHide => Some("Masquer les morphs qui ne bougent qu'un côté"),
        TextKey::AppearanceSearch => Some("Rechercher des looks"),
        TextKey::AppearanceLayers => Some("Calques d aspect"),
        TextKey::AddAppearanceLayer => Some("Ajouter l aspect selectionne"),
        TextKey::AppearanceLayersEmpty => Some("Aucun calque"),
        TextKey::AppearanceLayerRaise => Some("Monter"),
        TextKey::AppearanceLayerLower => Some("Descendre"),
        TextKey::LookFind => Some("Charger un look"),
        TextKey::CameraTrackballArmed => {
            Some("Trackball · faites glisser pour tourner librement · clic ou R pour quitter")
        }
        TextKey::HelpTrackball => Some("Rotation trackball"),
        TextKey::ShortcutTrackball => Some("R"),
        TextKey::SplitModelViewTooltip => Some("Voir sous deux angles — diviser la vue"),
        TextKey::SplitModelViewName => Some("Vue divisee"),
        TextKey::HelpLevelRoll => Some("Remettre l'horizon à plat"),
        TextKey::ShortcutLevelRoll => Some("Alt+R"),
        TextKey::RecoveryTitle => Some("La dernière session s'est terminée de façon inattendue"),
        TextKey::RecoveryBody => Some(
            "Vos morphs, la sculpture et les calques de texture ont été enregistrés juste avant l'arrêt. Les restaurer ?",
        ),
        TextKey::RecoveryRestore => Some("Restaurer"),
        TextKey::RecoveryDiscard => Some("Repartir de zéro"),
        TextKey::RecoveryRestored => Some("Session précédente restaurée."),
        TextKey::RecoveryLookMissing => Some(
            "Le look de cette session n'est pas encore listé — les textures sont restaurées et les modifications s'appliqueront à sa sélection.",
        ),
        TextKey::SettingsBrushSweepGroup => Some("Geste de taille du pinceau"),
        TextKey::BrushSweepBlender => Some("Blender"),
        TextKey::BrushSweepZBrush => Some("ZBrush"),
        TextKey::BrushSweepTooltip => {
            Some("Blender valide par un clic ou une seconde pression ; ZBrush au relâchement")
        }
        TextKey::HairPackageStyle => Some("Empaqueter en .var"),
        TextKey::HairPackageDone => Some("Cheveux empaquetés"),
        TextKey::HairPackageFailed => Some("Échec de l'empaquetage"),
        TextKey::HairPackageNeedsInstall => Some("Installez d'abord la coiffure"),
        TextKey::DialogOpenHeadPreset => Some("Ouvrir un préréglage de tête"),
        TextKey::FindHeadPreset => Some("Trouver un préréglage…"),
        TextKey::SettingsVignetteIntensity => Some("Quantité"),
        TextKey::SettingsVignetteSmoothness => Some("Douceur"),
        TextKey::SettingsVignetteRoundness => Some("Rondeur"),
        TextKey::SculptNotCarried => Some("La sculpture n'a pas pu suivre ce look"),
        TextKey::SettingsToneCurve => Some("Courbe tonale"),
        TextKey::SettingsToneCurveTooltip => Some("Ajuste les ombres et les hautes lumières"),
        TextKey::SettingsVignetteTooltip => Some("Assombrit les bords du cadre"),
        TextKey::SettingsEffectEnabled => Some("Activé"),
        TextKey::SettingsInterfaceGroup => Some("Interface"),
        TextKey::SettingsTooltips => Some("Afficher les infobulles"),
        TextKey::SettingsResetGroup => Some("Réinitialiser"),
        TextKey::SettingsClearCache => Some("Vider le cache"),
        TextKey::SettingsClearCacheTooltip => Some(
            "Bases extraites et banques de morphs, reconstruites au besoin. Les vider ne coûte qu'un démarrage plus lent.",
        ),
        TextKey::SettingsResetAll => Some("Réinitialiser tous les réglages et redémarrer"),
        TextKey::SettingsGraphicsLighting => Some("Éclairage"),
        TextKey::SettingsGraphicsEffects => Some("Effets"),
        TextKey::SettingsGraphicsQuality => Some("Qualité"),
        TextKey::SettingsMsaa => Some("MSAA"),
        TextKey::SettingsMsaaTooltip => Some(
            "Lisse les bords en escalier selon le nombre d'échantillons choisi. Plus haut est plus net et plus coûteux.",
        ),
        TextKey::SoloViewHint => Some("Afficher seulement cette partie"),
        TextKey::SoloViewRestoreHint => Some("Rétablir la vue précédente"),
        TextKey::ToneCurveFilmic => Some("Filmique"),
        TextKey::ToneCurveSoft => Some("Douce"),
        TextKey::LooksHiddenForOtherFigure => Some("Les looks de l'autre personnage sont masqués"),
        TextKey::LookBelongsToOtherFigure => Some("Ce look est conçu pour l'autre personnage"),
        TextKey::MorphEdit => Some("Éditer les morphs"),
        TextKey::VaMMorphPairImported => Some("Paire de morphs VaM importée"),
        TextKey::VaMMorphPairRejected => Some("Paire de morphs VaM non importable"),
        TextKey::MorphCategoryAll => Some("Tous"),
        TextKey::MorphCategoryEyes => Some("Yeux"),
        TextKey::MorphCategoryBrows => Some("Sourcils"),
        TextKey::MorphCategoryNose => Some("Nez"),
        TextKey::MorphCategoryMouth => Some("Bouche"),
        TextKey::MorphCategoryJaw => Some("Mâchoire"),
        TextKey::MorphCategoryCheeks => Some("Joues"),
        TextKey::MorphCategoryEars => Some("Oreilles"),
        TextKey::MorphCategoryHead => Some("Visage"),
        TextKey::MorphCategoryExpression => Some("Expression"),
        TextKey::NoMatchingMorphs => Some("Aucun morph correspondant"),
        TextKey::ResetMorphs => Some("Tout réinitialiser"),
        TextKey::LightRotationGesture => Some("Shift+Glisser clic droit"),
        TextKey::UpdateAvailable => Some("Mise à jour"),
        TextKey::PackageFromFiles => Some("Choisir des fichiers"),
        TextKey::PackageMorphFiles => Some("Morphs"),
        TextKey::PackageTextureFiles => Some("Textures"),
        TextKey::PackageListEmpty => Some("Rien d'ajouté pour l'instant"),
        TextKey::LightingStudio => Some("Studio"),
        TextKey::LightingSoft => Some("Doux"),
        TextKey::LightingDaylight => Some("Lumière du jour"),
        TextKey::LightingWarm => Some("Chaud"),
        TextKey::LightingGloss => Some("Contrôle du brillant"),
        TextKey::LightingPortrait => Some("Portrait"),
        TextKey::ImportLoadingMesh => Some("Chargement du maillage"),
        TextKey::ImportSimplifying => Some("Simplification du maillage"),
        TextKey::StageOptimizeHead => Some("Optimiser la tête"),
        TextKey::StagePrepareInput => Some("Préparer l'entrée"),
        TextKey::StageAlignScan => Some("Aligner le scan"),
        TextKey::StageFitShape => Some("Ajuster la forme G2"),
        TextKey::StageValidate => Some("Valider la structure"),
        TextKey::StagePrepareResult => Some("Préparer le résultat"),
        TextKey::HeavyMeshNote => {
            Some("environ {count} triangles -- plus il y en a, plus c'est long")
        }
        TextKey::TransformGroups => Some("Parties"),
        TextKey::CollapseTransformGroups => Some("Réduire les parties"),
        TextKey::ExpandTransformGroups => Some("Développer les parties"),
        TextKey::Skin => Some("Peau"),
        TextKey::SculptTear => Some("Film lacrymal"),
        TextKey::SculptLips => Some("Lèvres"),
        TextKey::SculptTeethTongue => Some("Dents · Langue"),
        TextKey::SculptInnerMouth => Some("Cavité buccale"),
        TextKey::SculptEyes => Some("Yeux"),
        TextKey::Size => Some("Taille"),
        TextKey::SculptXSymmetry => Some("Symétrie X"),
        TextKey::SculptXSymmetryTooltip => Some("Applique le même tracé des deux côtés"),
        TextKey::BackfaceProtection => Some("Faces arrière"),
        TextKey::AutoGroup => Some("Groupe auto"),
        TextKey::AutoGroupTooltip => {
            Some("Ne déforme qu'à l'intérieur d'un seul groupe de polygones")
        }
        TextKey::EyeTrackingAuto => Some("Auto"),
        TextKey::EyeGazeFreeze => Some("Figer en morph"),
        TextKey::EyeGazeFreezeTooltip => Some(
            "Verrouille le regard actuel dans les morphs, pour que l'onglet d'enregistrement exporte cette pose des yeux",
        ),
        TextKey::Reset => Some("Réinit."),
        TextKey::SculptGrab => Some("Attraper"),
        TextKey::SculptSmooth => Some("Lisser"),
        TextKey::SculptPushPull => Some("Pousser · Tirer"),
        TextKey::ShortcutSculptGrab => Some("Glisser"),
        TextKey::ShortcutSculptSmooth => Some("Maj + glisser"),
        TextKey::ShortcutSculptPushPull => Some("Ctrl / Alt + glisser"),
        TextKey::ResetSculpt => Some("Réinitialiser la déformation"),
        #[cfg(test)]
        TextKey::SculptApplied => Some("Modifications de sculpture appliquées"),
        TextKey::SculptFailed => Some("Échec de l'opération de sculpture"),
        TextKey::Ready => Some("Prêt"),
        TextKey::Busy => Some("L'édition est verrouillée pendant la génération"),
        TextKey::NeedScan => Some(
            "Ouvrez d'abord une tête scannée OBJ, GLB ou FBX, ou partez d'un look déjà installé dans VaM",
        ),
        TextKey::ScanLoadFailed => Some("Impossible de lire ce fichier de tête"),
        TextKey::NeedEyeMorph => Some("Le morph d'yeux intégré n'est pas compatible avec ce G2"),
        TextKey::ReviewEyePins => Some("Vérifiez les repères rouges des paupières"),
        TextKey::ResultUnavailable => Some("Ajustez d'abord le visage"),
        TextKey::MorphNotInLibrary => {
            Some("Ce morph n'est pas encore dans la bibliothèque ; le catalogue charge.")
        }
        TextKey::SculptNeedsBaseHead => {
            Some("Commencez d'abord avec la tête de base dans l'onglet Créer.")
        }
        TextKey::MorphUnavailable => Some("Aucun morph de résultat compatible n'est disponible"),
        TextKey::AlignmentPending => {
            Some("Chargez une tête scannée et sélectionnez votre dossier VaM")
        }
        TextKey::ScanLoaded => Some("Tête personnalisée OBJ, GLB ou FBX chargée"),
        TextKey::ScanUnloaded => Some("Fichier de tête retiré ; la base G2 est prête"),
        TextKey::PinPairsMismatched => {
            Some("Le nombre d'épingles ne correspond pas ; épingles sans paire")
        }
        TextKey::UnloadScanTooltip => Some("Retirer ce fichier de tête"),
        TextKey::TemplateLoaded => Some("G2 chargé"),
        TextKey::PinsReset => Some("Tous les repères ont été réinitialisés"),
        TextKey::ResultStale => Some("Les modifications exigent de régénérer le résultat"),
        TextKey::GenerationStarted => Some("Ajustement du visage"),
        TextKey::Cancelling => Some("Annulation de l'ajustement du visage"),
        TextKey::GenerationComplete => Some("Ajustement du visage terminé"),
        TextKey::GenerationCancelled => Some("Ajustement du visage annulé"),
        TextKey::GenerationFailed => Some("Échec de l'ajustement du visage"),
        TextKey::ExportStarted => Some("Exportation"),
        TextKey::ExportComplete => Some("Exportation terminée"),
        TextKey::ExportFailed => Some("Échec de l'exportation"),
        TextKey::MorphNameHint => Some("Saisissez un nom de morph"),
        TextKey::MorphGroupHint => Some("Groupe  (p. ex. Custom, Morphs/Morph Loader)"),
        TextKey::MorphRegionHint => Some("Région  (p. ex. Custom, Morphs/Body)"),
        TextKey::DirectEntry => Some("Saisir"),
        TextKey::VaMMetadataPoseMorph => Some("Déclarer comme Pose Morph"),
        TextKey::VaMMetadataPoseMorphTooltip => Some("Traité avec les expressions et les poses"),
        TextKey::VaMMetadataBoneCorrection => Some("Correction du squelette"),
        TextKey::VaMMetadataBoneCorrectionTooltip => {
            Some("Recentre les pivots sur le squelette remodelé")
        }
        TextKey::NativeCorePending => {
            Some("L'intégration du moteur d'ajustement natif est en cours")
        }
        TextKey::MorphPreviewUpdated => Some("Aperçu du morph mis à jour"),
        TextKey::MorphsReset => Some("Morphs réinitialisés"),
        TextKey::MorphsResetUndone => Some("Réinitialisation des morphs annulée"),
        TextKey::MorphEditUndone => Some("Ajustement du morph annulé"),
        TextKey::MorphApplied => Some("Morph appliqué"),
        TextKey::HelpPlace => Some("Poser un repère"),
        TextKey::HelpMove => Some("Déplacer un repère"),
        TextKey::HelpDelete => Some("Supprimer un repère"),
        TextKey::HelpXSymmetry => Some("Symétrie X des repères"),
        TextKey::HelpSnapRotation => Some("Rotation aimantée"),
        TextKey::HelpDragZoom => Some("Zoom au glisser"),
        TextKey::HelpOrbit => Some("Orbiter"),
        TextKey::HelpPan => Some("Panoramique"),
        TextKey::HelpZoom => Some("Zoom"),
        TextKey::HelpLight => Some("Pivoter la lumière"),
        TextKey::HelpFrameView => Some("Cadrer la tête"),
        TextKey::HelpUndo => Some("Annuler"),
        TextKey::ShortcutPlace => Some("Clic gauche"),
        TextKey::ShortcutMove => Some("Alt + glisser"),
        TextKey::ShortcutDelete => Some("Clic droit"),
        TextKey::ShortcutXSymmetry => Some("X"),
        TextKey::ShortcutSnapRotation => Some("Maj / Ctrl + glisser rotation"),
        TextKey::ShortcutDragZoom => Some("Ctrl + clic molette + glisser vertical"),
        TextKey::ShortcutOrbit => Some("Glisser bouton du milieu"),
        TextKey::ShortcutPan => Some("Maj + glisser bouton du milieu"),
        TextKey::ShortcutZoom => Some("Molette"),
        TextKey::ShortcutLight => Some("Maj + glisser droit"),
        TextKey::ShortcutFrameView => Some("F"),
        TextKey::HelpSnapView => Some("Vue aimantée"),
        TextKey::ShortcutSnapView => Some("Alt + glisser molette"),
        TextKey::HelpStandardViews => Some("Vues standard"),
        TextKey::HelpCameraProjection => Some("Perspective / Orthographique"),
        TextKey::ShortcutStandardViews => Some("Pavé num. 1-9"),
        TextKey::ShortcutCameraProjection => Some("Pavé numérique ."),
        TextKey::ShortcutUndo => Some("Ctrl+Z"),
        TextKey::HelpRedo => Some("Rétablir"),
        TextKey::ShortcutRedo => Some("Ctrl+Y"),
        TextKey::HairPresetSection => Some("Prereglages de cheveux"),
        TextKey::HairPresetLoad => Some("Charger"),
        TextKey::HairToolPick => Some("Choisir le calque"),
        TextKey::HairToolPickHint => Some(
            "Cliquez sur une meche dans la vue pour activer son calque. Le pinceau attend le clic suivant.",
        ),
        TextKey::HelpHairPick => Some("Choisir le calque sous le curseur"),
        TextKey::ShortcutHairPick => Some("V, ou Ctrl + clic avec tout pinceau"),
        TextKey::HelpHairSmooth => Some("Lisser au lieu de peigner"),
        TextKey::ShortcutHairSmooth => Some("Shift en peignant"),
        TextKey::HairGroupScalp => Some("Cuir chevelu"),
        TextKey::HairScalpAlpha => Some("Decoupe du cuir chevelu"),
        TextKey::HairScalpPanel => Some("Cuir chevelu"),
        TextKey::HairScalpMesh => Some("Maillage du cuir chevelu"),
        TextKey::HairScalpCreate => Some("Creer le cuir chevelu"),
        TextKey::HairScalpAbsent => {
            Some("Cette coiffure n'a pas encore de calque de cuir chevelu.")
        }
        TextKey::HistoryBranchTitle => Some("Les étapes suivantes seront perdues"),
        TextKey::HistoryBranchProceed => Some("Modifier quand même"),
        TextKey::DoNotShowAgain => Some("Ne plus afficher"),
        TextKey::SurfaceBaseUnblended => {
            Some("Certaines cartes PBR n'ont pas pu être fusionnées avec la peau VaM")
        }
        TextKey::DetailEditRedone => Some("La modification annulée a été rétablie"),
        TextKey::ShortcutBrushSmaller => Some("Pinceau plus petit"),
        TextKey::ShortcutBrushLarger => Some("Pinceau plus grand"),
        TextKey::ShortcutBrushSizeDrag => Some("Taille du pinceau"),
        TextKey::ShortcutBrushStrengthDrag => Some("Force du pinceau"),
        TextKey::ShortcutStencilCancel => Some("Annuler le placement du pochoir"),
        TextKey::ShortcutViewOrbit => Some("Orbiter la vue"),
        TextKey::ShortcutViewPan => Some("Déplacer la vue"),
        TextKey::ShortcutViewDolly => Some("Rapprocher ou éloigner la vue"),
        TextKey::TemplatePending => {
            Some("Sélectionnez votre dossier VaM pour préparer la base G2 automatiquement")
        }
        TextKey::ResultEmpty => Some("Aucun morph préparé"),
        TextKey::ScaleLink => Some("Lier l'échelle"),
        TextKey::ScaleLinkTooltip => Some("Lie les valeurs d'échelle X/Y/Z"),
        TextKey::CameraSettings => Some("Réglages de la caméra"),
        TextKey::Perspective => Some("Perspective"),
        TextKey::Orthographic => Some("Orthographique"),
        TextKey::Fov => Some("FOV"),
        TextKey::ResetCamera => Some("Réinitialiser la caméra"),
        TextKey::WindowMinimize => Some("Réduire"),
        TextKey::WindowMaximize => Some("Agrandir"),
        TextKey::WindowRestore => Some("Restaurer"),
        TextKey::WindowClose => Some("Fermer"),
        TextKey::TooltipShow => Some("Afficher"),
        TextKey::TooltipHide => Some("Masquer"),
        TextKey::TooltipLock => Some("Verrouiller"),
        TextKey::TooltipUnlock => Some("Déverrouiller"),
        TextKey::AdjustEyeGaze => Some("Régler le regard"),
        TextKey::ScanLoading => Some("Chargement de la tête scannée"),
        TextKey::TemplateLoading => Some("Chargement du gabarit G2"),
        TextKey::ResultLoading => Some("Chargement du maillage résultant"),
        TextKey::VaMMorphPairLoading => Some("Chargement de la paire de morphs VaM"),
        TextKey::SystemLog => Some("Journal système"),
        TextKey::Strength => Some("Force"),
        TextKey::StrengthShort => Some("Force"),
        TextKey::SculptBrushMove => Some("Attraper"),
        TextKey::SculptBrushSmooth => Some("Lisser"),
        TextKey::SculptBrushRestore => Some("Restaurer"),
        TextKey::SculptBrushMoveTooltip => {
            Some("Tire la surface avec le pointeur ; la force détermine à quel point elle suit")
        }
        TextKey::SculptBrushSmoothTooltip => Some("Lisse le détail de la surface"),
        TextKey::SculptBrushRestoreTooltip => Some("Ramène vers la forme de base"),
        TextKey::SculptBrushMask => Some("Masque"),
        TextKey::SculptBrushMaskTooltip => Some(
            "Creuse le calque d aspect selectionne pour reveler celui du dessous. Alt le repeint",
        ),
        TextKey::TextureLayers => Some("Calques de texture"),
        TextKey::PackageSection => Some("Exporter en VAR"),
        TextKey::PackageCreatorHint => Some("Auteur"),
        TextKey::PackageNameHint => Some("Nom du paquet"),
        TextKey::PackageDescriptionHint => Some("Description"),
        TextKey::PackageCreditsHint => Some("Crédits"),
        TextKey::PackageInstructionsHint => Some("Instructions"),
        TextKey::PackageLinkHint => Some("Lien de soutien"),
        TextKey::PackageSave => Some("Enregistrer le VAR"),
        TextKey::PackageFromThisHead => Some("Créer depuis cette tête"),
        TextKey::PackageAddMorphs => Some("Ajouter des morphs"),
        TextKey::PackageAddTextures => Some("Ajouter des textures"),
        TextKey::PackageNeedsVamRestart => Some("VaM doit être redémarré"),
        TextKey::PackageSavedTo => Some("Enregistré dans"),
        TextKey::FieldRequired => Some("(requis)"),
        TextKey::FieldOptional => Some("(facultatif)"),
        TextKey::PackageSaveTo => Some("Enregistrer dans"),
        TextKey::PackageNothingToPack => {
            Some("Rien à empaqueter. Activez cette tête ou ajoutez un fichier.")
        }
        TextKey::PackageVersionMarker => Some("V"),
        TextKey::PackageCreatorRequired => Some("Un nom d'auteur est requis."),
        TextKey::PackageNameRequired => Some("Un nom de paquet est requis."),
        TextKey::PackageReplaceTitle => Some("Un VAR de ce nom existe déjà"),
        TextKey::PackageReplaceBody => Some(
            "Le remplacer ? Les scènes qui référencent cette version liront le nouveau contenu.",
        ),
        TextKey::LicenseByTooltip => Some("créditer l'auteur"),
        TextKey::LicenseBySaTooltip => {
            Some("créditer l'auteur / partage dans les mêmes conditions")
        }
        TextKey::LicenseByNdTooltip => Some("créditer l'auteur / pas de modification"),
        TextKey::LicenseByNcTooltip => Some("créditer l'auteur / pas d'utilisation commerciale"),
        TextKey::LicenseByNcSaTooltip => Some(
            "créditer l'auteur / pas d'utilisation commerciale / partage dans les mêmes conditions",
        ),
        TextKey::LicenseByNcNdTooltip => {
            Some("créditer l'auteur / pas d'utilisation commerciale / pas de modification")
        }
        TextKey::LicensePcEaTooltip => {
            Some("accès anticipé pour les mécènes, pas de redistribution")
        }
        TextKey::AddTextureLayer => Some("Ajouter une image"),
        TextKey::AddG2UvTextureLayer => Some("Ajouter une UV"),
        TextKey::AddTextureLayerTooltip => {
            Some("Place une photo de visage sur la tete et l'edite comme texture")
        }
        TextKey::AddG2UvTextureLayerTooltip => {
            Some("Ajoute une texture face G2 dans sa propre disposition UV et l'edite")
        }
        TextKey::AddTextureLayerHint => {
            Some("Ajoutez une image de visage ou une texture G2 avec le bouton +.")
        }
        TextKey::BakingTextures => Some("Bake des textures…"),
        TextKey::BakeTexturesForSave => Some("Baker pour enregistrer"),
        TextKey::TextureMetalRough => Some("Metallic / Roughness"),
        TextKey::TextureGlossSmooth => Some("Glossiness / Smoothness"),
        TextKey::LayerOpacity => Some("Opacité du calque"),
        TextKey::PinOpacity => Some("Opacité des repères"),
        TextKey::TransferPins => Some("Transférer les repères"),
        TextKey::TransferPinsTooltip => {
            Some("Copie les repères posés vers les images des autres calques.")
        }
        TextKey::DeleteLayer => Some("Supprimer le calque"),
        TextKey::TextureNormalStrength => Some("Force de la normale"),
        TextKey::TextureInvertScalar => Some("Inverser les valeurs scalaires"),
        TextKey::BlendNormal => Some("Normal"),
        TextKey::BlendMultiply => Some("Produit"),
        TextKey::BlendScreen => Some("Superposition"),
        TextKey::BlendOverlay => Some("Incrustation"),
        TextKey::MatchToneToLayerBelow => Some("Harmoniser le ton avec le calque inférieur"),
        TextKey::ImageMirror => Some("Miroir gauche/droite"),
        TextKey::ImageMirrorOff => Some("Aucun"),
        TextKey::ImageMirrorToLeft => Some("Vers la gauche"),
        TextKey::ImageMirrorToRight => Some("Vers la droite"),
        TextKey::ImageAdjustments => Some("Réglages de l'image"),
        TextKey::Exposure => Some("Exposition"),
        TextKey::Contrast => Some("Contraste"),
        TextKey::Saturation => Some("Saturation"),
        TextKey::Hue => Some("Teinte"),
        TextKey::Temperature => Some("Température"),
        TextKey::TextureMaskPreviewTooltip => {
            Some("Affiche le masque du calque actif en surimpression rouge translucide")
        }
        TextKey::ClearLayerMask => Some("Effacer le masque du calque"),
        TextKey::ResetSourceRetouch => Some("Réinitialiser la retouche source"),
        TextKey::CloneSampleHint => {
            Some("Alt+clic sur l'image source pour choisir l'échantillon de clonage.")
        }
        TextKey::BakeBase => Some("Options de bake"),
        TextKey::TransparentOverlay => Some("Baker les calques seuls"),
        TextKey::CurrentSkinBake => Some("Baker avec la peau VaM"),
        TextKey::TransparentOverlayTooltip => {
            Some("Bake uniquement les images des calques, sans le préréglage de peau")
        }
        TextKey::CurrentSkinBakeTooltip => {
            Some("Bake les calques par-dessus le préréglage de peau servant de base")
        }
        TextKey::SkinPresetRequired => {
            Some("Choisissez d'abord un préréglage dans les réglages de peau")
        }
        TextKey::SkinSettings => Some("Réglages de peau"),
        TextKey::ScanHeadColor => Some("Couleur de la tête scannée"),
        TextKey::G2HeadColor => Some("Couleur de la tête G2"),
        TextKey::MorphFilterAll => Some("Tous"),
        TextKey::MorphFilterActive => Some("Actifs"),
        TextKey::BrushDodge => Some("Éclaircir"),
        TextKey::BrushBurn => Some("Assombrir"),
        TextKey::BrushSaturate => Some("Saturer"),
        TextKey::BrushDesaturate => Some("Désaturer"),
        TextKey::ShowLayersOnly => Some("Images des calques seuls"),
        TextKey::ShowLayersOnlyTooltip => {
            Some("Masque la peau VaM et n'affiche que les images des calques")
        }
        TextKey::MatchToneTooltip => {
            Some("Ajuste exposition, saturation et température sur le calque du dessous")
        }
        TextKey::ClearLayerMaskTooltip => Some("Efface le masque de ce calque"),
        TextKey::TextureMetalRoughTooltip => {
            Some("Enregistre les cartes en convention metallic/roughness")
        }
        TextKey::TextureGlossSmoothTooltip => {
            Some("Enregistre les cartes en convention glossiness/smoothness de VaM")
        }
        TextKey::TextureInvertScalarTooltip => Some(
            "Inverse les valeurs si la source suit la convention opposée (roughness ↔ glossiness)",
        ),
        TextKey::BaseWithoutSkin => Some("Calque"),
        TextKey::BaseWithoutSkinTooltip => Some(
            "Les calques peints sur la couleur unie, sans le préréglage ; la texture exportée est inchangée",
        ),
        TextKey::SolidColorTooltip => Some("La couleur unie seule, sans texture"),
        TextKey::TextureSkinTooltip => {
            Some("Les calques peints par-dessus le préréglage de peau VaM")
        }
        TextKey::BoundaryFeather => Some("Adoucir le bord du visage"),
        TextKey::BoundaryFeatherTooltip => Some(
            "Diffus uniquement. Les cartes comme la normale et la rugosité ne peuvent pas être fondues via l'alpha : leur bord reste net",
        ),
        TextKey::Baking => Some("Bake…"),
        TextKey::TextureToolPinPair => Some(
            "Paire de repères — posez des points correspondants sur le visage 3D et l'image source",
        ),
        TextKey::TextureToolMask => {
            Some("Masque — peignez pour révéler ; maintenez Alt pour masquer")
        }
        TextKey::TextureToolClone => {
            Some("Tampon de duplication — Alt+clic sur un échantillon, puis peignez pour le copier")
        }
        TextKey::TextureToolDodgeBurn => {
            Some("Éclaircir/assombrir — éclaircit ; maintenez Alt pour assombrir")
        }
        TextKey::TextureToolSponge => {
            Some("Éponge — ajoute de la saturation ; maintenez Alt pour l'enlever")
        }
        TextKey::SelectTextureLayer => Some("Sélectionnez un calque de texture."),
        TextKey::LoadingTextureImage => Some("Chargement de l'image…"),
        TextKey::ScanTextureAlignment => {
            Some("La texture du scan hérite de l'alignement 3D ajusté.")
        }
        TextKey::NoTextureImage => Some("Aucune image."),
        TextKey::MorphSavedReloadFemale => Some(
            "Appuyez sur Reload Custom Morphs dans l'onglet Female Morphs de VaM pour actualiser.",
        ),
        TextKey::MorphSavedReloadMale => Some(
            "Appuyez sur Reload Custom Morphs dans l'onglet Male Morphs de VaM pour actualiser.",
        ),
        TextKey::BootStarting => Some("Préparation de Vkit"),
        TextKey::BootFonts => Some("Chargement des polices"),
        TextKey::BootGraphics => Some("Initialisation des graphismes"),
        TextKey::BootSettings => Some("Chargement des réglages"),
        TextKey::BootTemplate => Some("Chargement de G2"),
        TextKey::BootWorkspace => Some("Préparation de l'espace de travail"),
        TextKey::BootReady => Some("Ouverture"),
        TextKey::StartupFailedTitle => Some("Vkit n'a pas pu démarrer"),
        TextKey::StartupFailedGraphics => Some(
            "Vkit dessine avec DirectX 12, et les graphismes de cette machine n'ont pas pu le fournir. Mettez à jour le pilote graphique, ou lancez Vkit sur une machine dotée d'un GPU compatible DirectX 12. Une session de bureau à distance ou une machine virtuelle n'a souvent pas d'affichage DirectX 12 propre.",
        ),
        TextKey::StartupFailedOther => {
            Some("Vkit s'est arrêté pendant le démarrage et n'a pas pu ouvrir sa fenêtre.")
        }
        TextKey::StartupFailedLogHint => Some("Ce qui a échoué a été écrit dans :"),
        TextKey::DialogOpenScan => Some("Ouvrir une tête OBJ, GLB ou FBX"),
        TextKey::DialogAddUvLayer => Some("Ajouter un calque de texture G2 UV"),
        TextKey::DialogAddTextureLayer => Some("Ajouter un calque de texture"),
        TextKey::DialogSaveMorphPair => Some("Enregistrer le morph VaM VMI + VMB"),
        TextKey::DialogChooseVamFolder => Some("Choisir le dossier Virt-A-Mate"),
        _ => None,
    }
}

pub(super) fn hair_label(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some("Quantité de cheveux"),
        "curveDensity" => Some("Densité de courbe"),
        "iterations" => Some("Itérations"),
        "simulationEnabled" => Some("Physique"),
        "collisionEnabled" => Some("Collision"),
        "weight" => Some("Poids"),
        "drag" => Some("Résistance de l'air"),
        "gravityMultiplier" => Some("Multiplicateur de gravité"),
        "collisionRadius" => Some("Rayon de collision"),
        "collisionRadiusRoot" => Some("Rayon de collision (racine)"),
        "friction" => Some("Frottement"),
        "bendResistance" => Some("Résistance à la flexion"),
        "rootRigidity" => Some("Rigidité à la racine"),
        "mainRigidity" => Some("Rigidité au milieu"),
        "tipRigidity" => Some("Rigidité à la pointe"),
        "jointRigidity" => Some("Rigidité des jointures"),
        "rigidityRolloffPower" => Some("Atténuation de rigidité"),
        "cling" => Some("Cohésion de style"),
        "clingRolloff" => Some("Atténuation de cohésion"),
        "snap" => Some("Accroche"),
        "usePaintedRigidity" => Some("Utiliser la rigidité peinte"),
        "width" => Some("Épaisseur"),
        "length1" => Some("Longueur 1"),
        "length2" => Some("Longueur 2"),
        "length3" => Some("Longueur 3"),
        "maxSpread" => Some("Dispersion maximale"),
        "spreadRoot" => Some("Dispersion (racine)"),
        "spreadMid" => Some("Dispersion (milieu)"),
        "spreadTip" => Some("Dispersion (pointe)"),
        "spreadMidpoint" => Some("Milieu de dispersion"),
        "spreadCurvePower" => Some("Courbe de dispersion"),
        "curlScale" => Some("Taille de boucle"),
        "curlFrequency" => Some("Fréquence de boucle"),
        "curlScaleRandomness" => Some("Aléa de taille"),
        "curlFrequencyRandomness" => Some("Aléa de fréquence"),
        "curlRoot" => Some("Boucle (racine)"),
        "curlMid" => Some("Boucle (milieu)"),
        "curlTip" => Some("Boucle (pointe)"),
        "curlMidpoint" => Some("Milieu de boucle"),
        "curlCurvePower" => Some("Courbe de boucle"),
        "curlX" => Some("Boucle X"),
        "curlY" => Some("Boucle Y"),
        "curlZ" => Some("Boucle Z"),
        "curlNormalAdjust" => Some("Ajustement de normale"),
        "curlAllowReverse" => Some("Autoriser le sens inverse"),
        "curlAllowFlipAxis" => Some("Autoriser l'axe inversé"),
        "Alpha Adjust" => Some("Opacité du cuir chevelu"),
        "Diffuse Color" => Some("Couleur diffuse du cuir"),
        "Gloss" => Some("Brillance du cuir"),
        "Specular Intensity" => Some("Spéculaire du cuir"),
        "Global Illumination Filter" => Some("Illumination globale du cuir chevelu"),
        "Diffuse Texture Offset" => Some("Décalage de texture du cuir"),
        "rootColor" => Some("Couleur de racine"),
        "tipColor" => Some("Couleur de pointe"),
        "specularColor" => Some("Couleur spéculaire"),
        "colorRolloff" => Some("Atténuation racine→pointe"),
        "randomColorPower" => Some("Force de couleur aléatoire"),
        "randomColorOffset" => Some("Décalage de couleur aléatoire"),
        "primarySpecularSharpness" => Some("Reflet primaire"),
        "secondarySpecularSharpness" => Some("Reflet secondaire"),
        "specularShift" => Some("Décalage du reflet secondaire"),
        "diffuseSoftness" => Some("Douceur diffuse"),
        "fresnelPower" => Some("Force de Fresnel"),
        "fresnelAttenuation" => Some("Atténuation de Fresnel"),
        "IBLFactor" => Some("Lumière indirecte"),
        "normalRandomize" => Some("Aléa des normales"),
        _ => None,
    }
}

pub(super) fn hair_hint(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some(
            "Combien de mèches poussent entre chaque trio de guides. C'est l'aspect des cheveux, et l'essentiel de leur coût.",
        ),
        "curveDensity" => Some(
            "Points tracés le long de chaque mèche. Plus il y en a, plus c'est lisse et lent ; la forme se lit bien avant que le nombre soit élevé.",
        ),
        "iterations" => Some(
            "Passes du solveur par image. Davantage tient les contraintes plus fermement dans les mouvements rapides.",
        ),
        "simulationEnabled" => {
            Some("Si cette partie bouge dans le jeu. Sans rapport avec l'interrupteur du viewport.")
        }
        "collisionEnabled" => Some("Si les mèches de cette partie sont repoussées hors du corps."),
        "weight" => Some(
            "Avec quel poids une mèche retombe. Amplifie la gravité et la résistance au déplacement.",
        ),
        "drag" => Some("Combien l'air retient une mèche. Élevé, elle se pose vite et oscille peu."),
        "gravityMultiplier" => {
            Some("La gravité sur cette partie, en multiple de celle de la scène.")
        }
        "collisionRadius" => {
            Some("L'épaisseur avec laquelle une mèche entre en collision, sur toute sa longueur.")
        }
        "collisionRadiusRoot" => {
            Some("L'épaisseur près de la racine, quand elle doit différer du reste.")
        }
        "friction" => Some("Combien une mèche agrippe ce qu'elle touche au lieu de glisser."),
        "bendResistance" => Some("Combien une mèche résiste à être pliée depuis la ligne droite."),
        "rootRigidity" => Some("Avec quelle force la racine tient la forme coiffée."),
        "mainRigidity" => Some("Avec quelle force le milieu tient la forme coiffée."),
        "tipRigidity" => Some("Avec quelle force la pointe tient la forme coiffée."),
        "jointRigidity" => Some(
            "La raideur des jointures de style : les ressorts qui gardent une queue de cheval en queue de cheval.",
        ),
        "rigidityRolloffPower" => Some(
            "Comment les trois rigidités se mêlent le long de la mèche. Plus haut porte la valeur de la racine plus loin.",
        ),
        "cling" => Some("Avec quelle force les jointures de style rapprochent les mèches."),
        "clingRolloff" => Some(
            "Jusqu'où la cohésion s'estompe le long de la mèche. Plus haut la confine près de la racine.",
        ),
        "snap" => Some("Avec quelle fermeté chaque segment garde la longueur qu'on lui a donnée."),
        "usePaintedRigidity" => {
            Some("Prendre la rigidité de la carte peinte plutôt que des trois curseurs.")
        }
        "width" => Some(
            "L'épaisseur de tracé d'une mèche. Le curseur de VaM s'arrête aussi à un dixième de millimètre.",
        ),
        "length1" => {
            Some("Longueur des mèches les plus proches du premier guide de leur triangle.")
        }
        "length2" => {
            Some("Longueur des mèches les plus proches du deuxième guide de leur triangle.")
        }
        "length3" => {
            Some("Longueur des mèches les plus proches du troisième guide de leur triangle.")
        }
        "maxSpread" => Some(
            "Un plafond sur l'écart d'une mèche à son guide, et non un gain. Dès qu'il dépasse l'espacement des guides, l'augmenter ne fait plus rien.",
        ),
        "spreadRoot" => Some(
            "Avec quelle liberté les mèches se séparent à la racine. Bas les rassemble vers le guide.",
        ),
        "spreadMid" => Some("Avec quelle liberté les mèches se séparent au milieu."),
        "spreadTip" => Some("Avec quelle liberté les mèches se séparent à la pointe."),
        "spreadMidpoint" => {
            Some("À partir de quel point de la mèche s'applique la dispersion du milieu.")
        }
        "spreadCurvePower" => {
            Some("Avec quelle brusquerie la dispersion tourne entre racine, milieu et pointe.")
        }
        "curlScale" => Some(
            "La largeur de la boucle : le rayon autour duquel elle s'enroule, pas sa longueur.",
        ),
        "curlFrequency" => Some("Tours par centimètre de mèche."),
        "curlScaleRandomness" => Some("Combien la largeur de boucle varie d'une mèche à l'autre."),
        "curlFrequencyRandomness" => {
            Some("Combien le rythme d'enroulement varie d'une mèche à l'autre.")
        }
        "curlRoot" => Some("Combien de boucle atteint la racine."),
        "curlMid" => Some("Combien de boucle atteint le milieu."),
        "curlTip" => Some("Combien de boucle atteint la pointe."),
        "curlMidpoint" => {
            Some("À partir de quel point de la mèche s'applique la boucle du milieu.")
        }
        "curlCurvePower" => {
            Some("Avec quelle brusquerie la boucle tourne entre racine, milieu et pointe.")
        }
        "curlX" => Some(
            "Le déplacement de la boucle selon X. Un vecteur que l'on fait tourner, pas un axe autour duquel s'enrouler.",
        ),
        "curlY" => Some("Le déplacement de la boucle selon Y."),
        "curlZ" => Some("Le déplacement de la boucle selon Z."),
        "curlNormalAdjust" => Some(
            "Ajouté au décalage de boucle selon la normale. Il soulève la spirale de la tête au lieu de l'incliner.",
        ),
        "curlAllowReverse" => Some(
            "Laisse certaines mèches s'enrouler à l'envers, pour qu'une tête bouclée ne se lise pas comme une mèche répétée.",
        ),
        "curlAllowFlipAxis" => {
            Some("Laisse certaines mèches s'enrouler autour d'un axe inversé, pour la même raison.")
        }
        "Alpha Adjust" => Some(
            "L'opacité de la calotte du cuir chevelu. La plupart des parties portent la calotte d'une autre et la gardent au minimum pour la cacher ; ne la montez que si une raie doit laisser voir le cuir.",
        ),
        "Diffuse Color" => Some(
            "La couleur diffuse de la calotte elle-même. VaM la livre en blanc, pour que la peau en dessous transparaisse.",
        ),
        "Gloss" => Some("La brillance de la calotte là où elle se voit."),
        "Specular Intensity" => Some("Avec quelle force la calotte accroche un reflet."),
        "Global Illumination Filter" => Some("Quantité de lumière ambiante captée par la calotte."),
        "Diffuse Texture Offset" => Some(
            "Ajouté à la couleur diffuse de la calotte. Malgré le nom, ce n'est pas un indice de planche.",
        ),
        "rootColor" => Some("La couleur à la racine."),
        "tipColor" => Some("La couleur à la pointe."),
        "specularColor" => Some("La couleur du reflet."),
        "colorRolloff" => {
            Some("Comment la couleur de racine cède à celle de pointe le long de la mèche.")
        }
        "randomColorPower" => Some("Avec quelle force les mèches varient de couleur entre elles."),
        "randomColorOffset" => {
            Some("De quel côté penche cette variation, plus claire ou plus sombre.")
        }
        "primarySpecularSharpness" => {
            Some("Le resserrement du reflet principal. Plus haut est plus lustré.")
        }
        "secondarySpecularSharpness" => {
            Some("Le resserrement du second reflet : celui, coloré, qui voyage avec les cheveux.")
        }
        "specularShift" => {
            Some("À quelle distance du premier le second reflet se place le long de la mèche.")
        }
        "diffuseSoftness" => Some(
            "Avec quelle douceur la lumière enveloppe une mèche au lieu de n'éclairer que le côté exposé.",
        ),
        "fresnelPower" => Some(
            "Avec quelle brusquerie le bord s'éclaire là où les cheveux se détournent du regard.",
        ),
        "fresnelAttenuation" => Some("La force de cet éclairement du bord."),
        "IBLFactor" => Some("Quelle part de la lumière ambiante de la scène les cheveux captent."),
        "normalRandomize" => Some(
            "De combien la normale d'ombrage de chaque mèche est bousculée, pour que les voisines ne s'ombrent pas à l'identique.",
        ),
        _ => None,
    }
}
