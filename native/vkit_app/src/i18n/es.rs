use super::TextKey;

pub(super) const fn label(key: TextKey) -> Option<&'static str> {
    #[allow(unreachable_patterns, reason = "the fallback outlives today's key set")]
    match key {
        TextKey::SurfaceSmooth => Some("Suavizado de malla"),
        TextKey::SurfaceSmoothTooltip => Some(
            "Suavizado de render compatible con VaM. 0 es la malla base editable, 3 es el valor predeterminado de VaM y 4 el máximo. Los morphs, las UV, el esculpido y el horneado de texturas conservan la topología base",
        ),
        TextKey::FaceMatch => Some("Crear"),
        TextKey::ResultPreview => Some("Guardar"),
        TextKey::Save => Some("Guardar"),
        TextKey::DetailCorrection => Some("Esculpir"),
        TextKey::TextureStage => Some("Textura"),
        TextKey::HairStage => Some("Cabello"),
        TextKey::HairUnavailable => {
            Some("Termina el resultado de esculpido antes de editar el cabello")
        }
        TextKey::HairPartsPanel => Some("Partes de cabello"),
        TextKey::AddHairPart => Some("Añadir parte de cabello"),
        TextKey::HairScalpMeshCrowded => {
            Some("Algunos mechones no cabían en la nueva malla de cuero cabelludo y se descartaron")
        }
        TextKey::HairScalpMissing => {
            Some("Falta la malla del cuero cabelludo; abre una raíz de VaM y vuelve a escanear")
        }
        TextKey::HairShowPoints => Some("Mostrar puntos"),
        TextKey::HairShowPointsHint => Some(
            "Muestra los puntos de plantado del cuero cabelludo. Apagalo para ver solo el pelo; los pinceles siguen funcionando.",
        ),
        TextKey::HairPartTint => Some("Tintar partes"),
        TextKey::HairPartTintHint => Some(
            "Da a cada parte un tinte distinto en el visor para distinguirlas. Solo visual; no afecta a la exportación.",
        ),
        TextKey::HairSettingsPanel => Some("Ajustes de pelo"),
        TextKey::HairCreateFirst => Some("Crea algo de pelo primero."),
        TextKey::HairHideStrands => Some("Ocultar pelo"),
        TextKey::HairHideStrandsHint => Some(
            "Quita el pelo generado para leer solo los mechones guia. Activarlo activa tambien los mechones.",
        ),
        TextKey::HairShowStreams => Some("Ver mechones guia"),
        TextKey::HairShowStreamsHint => {
            Some("Dibuja el mechon que editas, sobre el pelo que genera.")
        }
        TextKey::HairViewportPhysics => Some("Física del visor"),
        TextKey::HairViewportPhysicsHint => {
            Some("Previsualiza la fisica en pantalla. No afecta a la exportacion.")
        }
        TextKey::HairMirrorEdit => Some("Edición en espejo"),
        TextKey::HairMirrorEditHint => {
            Some("Los pinceles editan ambos lados a la vez, en simetría izquierda-derecha.")
        }
        TextKey::HairAutoPart => Some("Parte automática"),
        TextKey::HairAutoPartHint => Some(
            "El pincel edita solo la parte reconocida bajo el cursor, en lugar de las capas activas.",
        ),
        TextKey::HairExportSection => Some("Guardar pelo"),
        TextKey::HairExportBothSexes => Some("Ambos"),
        TextKey::HairExportRescanNotice => {
            Some("Pulsa VaM - Hair - Rescan Files para verlo en la lista")
        }
        TextKey::HairExportOverwroteNotice => {
            Some("Una miniatura sobrescrita aparece tras Hard Reset o reiniciar VaM")
        }
        TextKey::HairSimToggleHint => Some("Ejecuta la fisica del pelo aqui y en el juego."),
        TextKey::HairCollisionToggleHint => {
            Some("Evita que el pelo atraviese la cabeza y el cuerpo.")
        }
        TextKey::HairStyleJoints => Some("Uniones de estilo"),
        TextKey::HairStyleJointsHint => Some(
            "Ata mechones cercanos para que el peinado mantenga su forma en el juego; es lo que escala Cling. Para pelo largo o recogido; añade peso al archivo.",
        ),
        TextKey::HairThumbnailPrompt => Some("Encuadra el ángulo y haz clic para capturar"),
        TextKey::HairThumbnailShoot => Some("Capturar"),
        TextKey::HairThumbnailSkip => Some("Omitir"),
        TextKey::HairThumbnailSaved => Some("Miniatura guardada"),
        TextKey::HairThumbnailFailed => Some("Fallo al guardar la miniatura"),
        TextKey::HairGameOnly => Some(
            "Ajuste de física: el visor no simula, así que solo actúa dentro de VaM. Se exporta con el estilo.",
        ),
        TextKey::HairPartVisible => Some("Mostrar u ocultar esta parte de cabello"),
        TextKey::RemoveHairPart => Some("Quitar parte de cabello"),
        TextKey::HairRenamePart => Some("Haz clic para renombrar"),
        TextKey::HairToolPlant => Some("Plantar"),
        TextKey::HairToolErase => Some("Borrar"),
        TextKey::HairToolGrow => Some("Crecer (Alt acorta)"),
        TextKey::HairToolComb => Some("Peinar"),
        TextKey::HairToolPlantHint => {
            Some("Planta mechones en los vertices del cuero cabelludo bajo el pincel")
        }
        TextKey::HairToolGrowHint => {
            Some("Alarga los mechones bajo el pincel; manten Alt para acortarlos")
        }
        TextKey::HairToolEraseHint => Some("Elimina los mechones bajo el pincel"),
        TextKey::HairToolCombHint => {
            Some("Arrastra los mechones bajo el pincel; las raices no se mueven")
        }
        TextKey::HairToolPinch => Some("Agrupar"),
        TextKey::HairToolPinchHint => Some("Junta los mechones bajo el pincel. Alt los separa"),
        TextKey::HairToolCut => Some("Cortar"),
        TextKey::HairToolCutHint => Some("Corta los mechones donde el pincel los cruza"),
        TextKey::HairToolPuff => Some("Ahuecar"),
        TextKey::HairToolPuffHint => Some("Levanta los mechones bajo el pincel. Alt los aplana"),
        TextKey::HairToolRigidity => Some("Rigidez"),
        TextKey::HairToolRigidityHint => Some(
            "El pintado de rigidez de VaM: con qué fuerza cada nodo vuelve a la pose que dibujaste. Lo pintado duro conserva su forma y se balancea como una pieza; lo pintado suelto cuelga libre. Alt afloja. Verde es sujeto, rojo es libre.",
        ),
        TextKey::HairToolVertex => Some("Punto"),
        TextKey::HairToolVertexHint => {
            Some("Agarra una articulación de un mechón y muévela; la longitud se conserva")
        }
        TextKey::HairCopySettings => Some("Copiar todo"),
        TextKey::HairPasteSettings => Some("Pegar todo"),
        TextKey::HairGroupPerformance => Some("Rendimiento"),
        TextKey::HairGroupPhysics => Some("Fisica"),
        TextKey::HairGroupStiffness => Some("Rigidez"),
        TextKey::HairGroupShape => Some("Forma"),
        TextKey::HairGroupCurl => Some("Rizo"),
        TextKey::HairGroupLook => Some("Aspecto"),
        TextKey::HairColorScalp => Some("Cuero"),
        TextKey::HairColorRoot => Some("Raiz"),
        TextKey::HairColorTip => Some("Punta"),
        TextKey::HairColorSpecular => Some("Brillo"),
        TextKey::HairScalpMask => Some("Textura del cuero"),
        TextKey::HairScalpMaskBuiltIn => Some("Integrada"),
        TextKey::HairScalpMaskCustom => Some("Desde archivo..."),
        TextKey::HairExport => Some("Exportar a VaM"),
        TextKey::HairExportName => Some("Nombre del objeto"),
        TextKey::HairExportCreator => Some("Creador"),
        TextKey::HairExportNeedsMetadata => Some("Indica el nombre y el creador antes de exportar"),
        TextKey::HairOverwriteTitle => Some("Reemplazar este estilo?"),
        TextKey::HairOverwriteBody => {
            Some("Ya hay un estilo con este nombre y creador. Exportar reemplaza sus archivos.")
        }
        TextKey::HairOverwriteProceed => Some("Reemplazar"),
        TextKey::HairExportDone => Some("Objeto de cabello guardado"),
        TextKey::HairExportFailed => Some("Error al exportar el cabello"),
        TextKey::HairExportNeedsPart => Some("Selecciona primero una parte con mechones"),
        TextKey::HairExportNeedsVaMRoot => Some("Configura una raiz de VaM antes de exportar"),
        TextKey::HairMirrorPart => Some("Reflejar la parte al otro lado"),
        TextKey::HairDuplicatePart => Some("Duplicar parte"),
        TextKey::HairSegmentsShort => Some("Seg"),
        TextKey::MorphCategoryBody => Some("Cuerpo"),
        TextKey::TextureNamePlaceholder => Some("Escribe un nombre de textura"),
        TextKey::TextureOverwriteTitle => Some("Esto reemplaza las texturas con el mismo nombre"),
        TextKey::OpenScan => Some("Escaneo"),
        TextKey::SourceMorphMissing => Some("Ningún morph de este look está instalado"),
        TextKey::EditDetails => Some("Editar detalles"),
        TextKey::Female => Some("Mujer"),
        TextKey::Male => Some("Hombre"),
        TextKey::Lighting => Some("Iluminación"),
        TextKey::Brightness => Some("Brillo"),
        TextKey::LightRotation => Some("Rotación de luz"),
        TextKey::AddHeadFileHint => Some("o arrastra aquí un archivo OBJ, GLB o FBX"),
        TextKey::AddHeadFile => Some("Añadir un archivo de cabeza"),
        TextKey::AutoAlign => Some("Autoalinear"),
        TextKey::PlacementReset => Some("Restablecer colocación"),
        TextKey::Scale => Some("Escala"),
        TextKey::Position => Some("Posición"),
        TextKey::Rotation => Some("Rotación"),
        TextKey::XMirror => Some("Espejo X"),
        TextKey::XMirrorTooltip => {
            Some("Al colocar un punto también se coloca su gemelo en el otro lado")
        }
        TextKey::NumbersTooltip => Some("Numera cada punto en el orden en que se colocó"),
        TextKey::XMirrorRejected => {
            Some("Se desactivó el espejo X porque no se encontró una contraparte segura")
        }
        TextKey::ScanFidelity => Some("Fidelidad al escaneo"),
        TextKey::ScanOptions => Some("Opciones de escaneo"),
        TextKey::RestoreNeckEars => Some("Restaurar cuello y orejas"),
        TextKey::RestoreNeckEarsTooltip => Some(
            "Mantiene orejas, nuca y parte posterior del cráneo en la base G2, mezclando el ajuste solo hacia la cara",
        ),
        TextKey::RestoreNeckEarsDeferred => {
            Some("Hay trazos de esculpido; el cambio se aplicará en la próxima generación")
        }
        TextKey::ScanFidelityTooltip => Some(
            "Por encima de 1 el ajuste sigue más la forma del escaneo y conserva menos la suavidad de la figura. Hasta 2 no hace nada más; pasado 2, el ruido del escaneo —motas, agujeros, costuras de unión— también empieza a aparecer como bultos, algo que puede merecer la pena con un escaneo limpio y denso. Los puntos nunca se sacrifican en ningún valor y no se relaja ninguna protección.",
        ),
        TextKey::ScanSymmetry => Some("Simetría del escaneo"),
        TextKey::Original => Some("Original"),
        TextKey::Left => Some("Izquierda"),
        TextKey::Right => Some("Derecha"),
        TextKey::EyesOpen => Some("Ojos abiertos"),
        TextKey::EyesClosed => Some("Ojos cerrados"),
        TextKey::View => Some("Opciones de vista"),
        TextKey::On => Some("Activo"),
        TextKey::Numbers => Some("Números"),
        TextKey::PinPairsPlaced => Some("Puntos"),
        TextKey::PinPlacement => Some("Colocación de puntos"),
        TextKey::ScanReference => Some("Referencia del escaneo"),
        TextKey::AddHeadFileAction => Some("Añadir un archivo de cabeza"),
        TextKey::AddHeadFileActionTooltip => {
            Some("Carga un archivo de cabeza escaneada (OBJ · GLB · FBX)")
        }
        TextKey::ContinueStep => Some("Continuar con la cabeza base"),
        TextKey::ContinueStepTooltip => Some(
            "Edita la cabeza G2 tal cual, sin escaneo; luego se puede cargar y ajustar un escaneo",
        ),
        TextKey::ResetAllPins => Some("Restablecer puntos"),
        TextKey::ResetAll => Some("Restablecer todo"),
        TextKey::MorphSave => Some("Guardar morph"),
        TextKey::TextureSaveSection => Some("Guardar texturas"),
        TextKey::Generate => Some("Ajustar cara"),
        TextKey::Cancel => Some("Cancelar"),
        TextKey::CopyAll => Some("Copiar todo"),
        TextKey::Next => Some("Guardar"),
        TextKey::TextureSkin => Some("Preajuste"),
        TextKey::SolidColor => Some("Sólido"),
        TextKey::BackfaceProtectionTooltip => Some("Protege el reverso de las mallas finas"),
        TextKey::WireframeColor => Some("Color del wireframe"),
        TextKey::WireframeSettings => Some("Wireframe"),
        TextKey::XraySettings => Some("Rayos X"),
        TextKey::ViewportLightingTooltip => Some("Ajusta la iluminación"),
        TextKey::Background => Some("Fondo"),
        TextKey::ViewportBackgroundTooltip => Some("Ajusta el fondo y la imagen de referencia"),
        TextKey::BackgroundRadial => Some("Radial"),
        TextKey::BackgroundVertical => Some("Vertical"),
        TextKey::BackgroundFlat => Some("Plano"),
        TextKey::ViewportWireframeTooltip => Some("Muestra y ajusta la superposición de wireframe"),
        TextKey::ViewportXrayTooltip => Some("Muestra y ajusta la superposición de rayos X"),
        TextKey::ViewportSkinTooltip => Some("Elige una textura de piel de VaM"),
        TextKey::FalloffTooltip => Some("Elige la curva de influencia del pincel"),
        TextKey::FalloffSmooth => Some("Suave"),
        TextKey::FalloffSmoother => Some("Más suave"),
        TextKey::FalloffSharp => Some("Agudo"),
        TextKey::FalloffLinear => Some("Lineal"),
        TextKey::PlaceHead => Some("Colocar la cabeza"),
        TextKey::PlaceHeadTooltip => Some(
            "Superpone la cabeza cargada sobre G2 para alinear su posición, rotación y tamaño.",
        ),
        TextKey::ScanOverlay => Some("Superponer escaneo"),
        TextKey::OverlayDone => Some("Terminar superposición"),
        TextKey::ProjectImage => Some("Proyectar"),
        TextKey::ProjectionCanvasHint => Some("Pinta en la vista 3D y aparecerá en este lienzo UV"),
        TextKey::HelpProjectionPaint => Some("Pintar (proyectar)"),
        TextKey::ShortcutProjectionPaint => Some("Arrastrar izq."),
        TextKey::HelpProjectionMove => Some("Mover imagen"),
        TextKey::ShortcutProjectionMove => Some("Arrastrar der."),
        TextKey::HelpProjectionRotate => Some("Rotar imagen"),
        TextKey::ShortcutProjectionRotate => Some("Mayús + arrastrar der."),
        TextKey::HelpProjectionZoom => Some("Escalar imagen"),
        TextKey::ShortcutProjectionZoom => Some("Rueda"),
        TextKey::HelpBrushSize => Some("Tamaño del pincel"),
        TextKey::ShortcutBrushSize => Some("Arrastrar F · [ ]"),
        TextKey::HelpBrushStrength => Some("Intensidad del pincel"),
        TextKey::ShortcutBrushStrength => Some("Shift+F arrastrar"),
        TextKey::HelpProjectionDone => Some("Terminar proyección"),
        TextKey::ShortcutProjectionDone => Some("Listo · Esc"),
        TextKey::ProjectDone => Some("Listo"),
        TextKey::ProjectDoneTooltip => {
            Some("Termina la proyección; todo lo pintado permanece en la capa")
        }
        TextKey::MorphCompare => Some("Comparar"),
        TextKey::Opacity => Some("Opacidad"),
        TextKey::G2Head => Some("Cabeza G2"),
        TextKey::Eyelashes => Some("Pestañas"),
        TextKey::OverwriteTitle => Some("Sobrescribir archivo"),
        TextKey::OverwriteBody => {
            Some("Ya existe un archivo con el mismo nombre. ¿Sobrescribirlo?")
        }
        TextKey::Overwrite => Some("Sobrescribir"),
        TextKey::ScanTextureLayer => Some("Textura del escaneo"),
        TextKey::PinPrompt => Some("Coloca un punto en cada uno, o ajusta sin ellos"),
        TextKey::TextureNeedsImage => Some("Añada una imagen desde el panel lateral"),
        TextKey::TexturePinPairPrompt => Some("Coloque pines emparejados a izquierda y derecha"),
        TextKey::TextureToolNeedsPins => Some("Coloca primero los puntos para usarlo"),
        TextKey::SymmetrySuggestion => Some("Se recomienda el espejo para un ajuste más limpio"),
        TextKey::SymmetryChangeTitle => Some("Se restablecerán los puntos"),
        TextKey::SymmetryChangeConfirm => Some("Confirmar"),
        TextKey::EyeClosure => Some("Cerrar ojos"),
        TextKey::VaMFolder => Some("Carpeta de VaM"),
        #[cfg(test)]
        TextKey::VaMGeometryBaseReady => Some("Base de geometría de VaM verificada"),
        TextKey::VaMGeometryBaseRejected => Some("No se puede usar la base de geometría de VaM"),
        TextKey::VaMGeometryBaseHowTo => Some(
            "Selecciona tu carpeta de VaM; la base neutra G2 se extrae automáticamente de tu instalación",
        ),
        TextKey::VaMBaseSourceUnityBundle => {
            Some("Base neutra integrada de VaM (extraída automáticamente)")
        }
        TextKey::VaMBaseSourceDazAnchorPair => Some("Ancla DAZ + base OBJ verificada"),
        TextKey::VaMBaseSourceUnityAnchorPair => {
            Some("Ancla neutra extraída + base OBJ verificada")
        }
        TextKey::VaMBaseSourceUnverifiedObj => Some("Base OBJ sin verificar"),
        TextKey::VaMUvMappingUnavailable => Some(
            "El mapeo UV de la piel de VaM no está listo. Revisa tu carpeta de VaM y actualiza el catálogo de pieles",
        ),
        TextKey::VaMIndexing => Some("Indexando el catálogo de VaM"),
        TextKey::VaMFailed => Some("Error en el catálogo de VaM"),
        TextKey::RefreshSkins => Some("Actualizar biblioteca de pieles"),
        TextKey::MorphLoading => Some("Cargando morph"),
        TextKey::MorphLoadFailed => Some("Error al cargar el morph"),
        TextKey::DefaultSkinMissing => Some("No se encontró esa piel. Revisa la carpeta de VaM"),
        TextKey::DefaultSkinSet => Some("Abrir con esta piel"),
        TextKey::DefaultSkinClear => Some("Dejar de abrir con esta piel"),
        TextKey::SkinNone => Some("Ninguna"),
        TextKey::SkinSearch => Some("Buscar pieles"),
        TextKey::SkinLoading => Some("Cargando piel"),
        TextKey::SkinReady => Some("Vista previa de piel lista"),
        TextKey::SkinLoadFailed => Some("Error al cargar la piel"),
        TextKey::SkinUvUnavailable => Some("UV de piel no disponible"),
        TextKey::NoMatchingSkins => Some("No hay pieles coincidentes"),
        TextKey::HairSearch => Some("Buscar pelo"),
        TextKey::HairReady => Some("Vista previa de pelo lista"),
        TextKey::HairLoadFailed => Some("Error al cargar el pelo"),
        TextKey::HairPartsSkipped => Some("Se muestra sin algunas partes"),
        TextKey::HairParts => Some("partes"),
        TextKey::BaseFace => Some("Cara base"),
        TextKey::Settings => Some("Ajustes"),
        TextKey::SettingsGraphics => Some("Gráficos"),
        TextKey::SettingsViewport => Some("Vista 3D"),
        TextKey::SettingsGeneral => Some("General"),
        TextKey::SettingsAbout => Some("Acerca de"),
        TextKey::SettingsShortcuts => Some("Atajos"),
        TextKey::ShortcutsCapturing => Some("Pulsa…"),
        TextKey::ShortcutsResetAll => Some("Restablecer todos los atajos"),
        TextKey::ShortcutGroupSystem => Some("En todas partes"),
        TextKey::ShortcutGroupAlignment => Some("Colocar la cabeza"),
        TextKey::ShortcutGroupSculpt => Some("Pinceles de escultura"),
        TextKey::ShortcutGroupHair => Some("Pinceles de pelo"),
        TextKey::ShortcutGroupView => Some("Vistas de cámara"),
        TextKey::ShortcutGroupNavigation => Some("Cambiar de pestaña"),
        TextKey::ShortcutViewReset => Some("Restablecer cámara"),
        TextKey::ShortcutViewProjection => Some("Perspectiva / ortográfica"),
        TextKey::ShortcutViewFront => Some("Vista frontal"),
        TextKey::ShortcutViewLeftSide => Some("Vista izquierda"),
        TextKey::ShortcutViewRightSide => Some("Vista derecha"),
        TextKey::ShortcutViewTop => Some("Vista superior"),
        TextKey::ShortcutViewBottom => Some("Vista inferior"),
        TextKey::ShortcutViewFrontUpperLeft => Some("Frontal superior izquierda"),
        TextKey::ShortcutViewFrontUpperRight => Some("Frontal superior derecha"),
        TextKey::ShortcutViewFrontLowerLeft => Some("Frontal inferior izquierda"),
        TextKey::ShortcutViewFrontLowerRight => Some("Frontal inferior derecha"),
        TextKey::ShortcutTabFaceMatch => Some("Pestaña de ajuste facial"),
        TextKey::ShortcutTabDetail => Some("Pestaña de detalle"),
        TextKey::ShortcutTabTexture => Some("Pestaña de textura"),
        TextKey::ShortcutTabHair => Some("Pestaña de pelo"),
        TextKey::ShortcutTabSave => Some("Pestaña de guardado"),
        TextKey::ShortcutGroupLists => Some("Capas y partes"),
        TextKey::ShortcutGroupVertex => Some("Nudos de mechón"),
        TextKey::ShortcutVertexAdd => Some("Mantener para añadir a la selección"),
        TextKey::ShortcutVertexRemove => Some("Mantener para quitar de la selección"),
        TextKey::ShortcutGroupTexture => Some("Lienzo de textura"),
        TextKey::ReferenceImages => Some("Referencia"),
        TextKey::ReferenceImagesEmpty => Some("Sin imágenes de referencia"),
        TextKey::ReferenceImageAdd => Some("Añadir una imagen"),
        TextKey::ReferenceImageRemove => Some("Quitar"),
        TextKey::ReferenceImageRaise => Some("Traer adelante"),
        TextKey::ReferenceImageLower => Some("Enviar atrás"),
        TextKey::ShortcutTextureCanvasPan => Some("Desplazar el lienzo (mantener)"),
        TextKey::ShortcutDiagnosticLogCopy => Some("Copiar el registro"),
        TextKey::ShortcutLightRotate => Some("Girar la luz (mantener)"),
        TextKey::ShortcutLayerHide => Some("Ocultar capa"),
        TextKey::ShortcutLayerUnhideAll => Some("Mostrar todas las capas"),
        TextKey::ShortcutLayerIsolate => Some("Ocultar las demás capas"),
        TextKey::ShortcutLayerLocalView => Some("Ver solo este (alternar)"),
        TextKey::ShortcutLayerInvertSelection => Some("Invertir la selección"),
        TextKey::ShortcutSculptSmoothHold => Some("Mantener para suavizar"),
        TextKey::ShortcutSculptInflateHold => Some("Mantener para inflar"),
        TextKey::ShortcutSculptAlternateHold => Some("Mantener para invertir"),
        TextKey::ShortcutHairSmoothHold => Some("Mantener para suavizar el pelo"),
        TextKey::ShortcutHairInvertHold => Some("Mantener para invertir la herramienta de pelo"),
        TextKey::ShortcutTextureInvertHold => {
            Some("Mantener para invertir la herramienta de textura")
        }
        TextKey::ShortcutListAddToSelection => Some("Mantener para añadir a la selección"),
        TextKey::ShortcutListSolo => Some("Mantener para aislar"),
        TextKey::ShortcutsExport => Some("Guardar el mapa en un archivo"),
        TextKey::ShortcutsImport => Some("Cargar un archivo de mapa"),
        TextKey::ShortcutsTaken => Some("Otra acción ya responde a eso"),
        TextKey::SettingsAboutBuild => Some("Compilación"),
        TextKey::SettingsAboutLicense => Some("Licencia"),
        TextKey::SettingsAboutDiagnostics => Some("Diagnóstico"),
        TextKey::SettingsTooltipsTooltip => {
            Some("Ayuda como esta, que es justo lo que acaba de pasar")
        }
        TextKey::AboutTagline => {
            Some("Una herramienta complementaria para crear morphs de cabeza de VaM.")
        }
        TextKey::AboutNoAffiliation => Some(
            "Virt-A-Mate pertenece a MeshedVR y Genesis 2 a DAZ 3D. Vkit es una herramienta independiente, sin vínculo con ninguno.",
        ),
        TextKey::AboutNoWarranty => Some(
            "Se ofrece tal cual, sin garantía. Lo que crees con él y la licencia de lo que cargues son responsabilidad tuya.",
        ),
        TextKey::AboutSupport => Some("Apoyo"),
        TextKey::AboutReportProblem => Some(
            "Vkit escribe un registro en cada ejecución, y un archivo aparte si se cae. Abre la carpeta y adjunta ambos al informe: un fallo que nadie más puede ver es un fallo que nadie puede arreglar.",
        ),
        TextKey::AboutOpenLogs => Some("Abrir la carpeta de registros"),
        TextKey::AboutReportIssue => Some("Informar de un problema"),
        TextKey::SettingsLightingPreset => Some("Preajuste"),
        TextKey::SettingsExposure => Some("Exposición"),
        TextKey::SettingsSmoothPasses => Some("Suavizado de superficie"),
        TextKey::SettingsSmoothPassesHint => {
            Some("Solo suaviza la visualización, igual que VaM. La forma guardada no cambia.")
        }
        TextKey::SettingsBackgroundGroup => Some("Fondo"),
        TextKey::SettingsBackground => Some("Estilo"),
        TextKey::SettingsOverlayGroup => Some("Superposiciones"),
        TextKey::SettingsWireframeColor => Some("Color del wireframe"),
        TextKey::SettingsWireframeOpacity => Some("Opacidad del wireframe"),
        TextKey::SettingsXrayOpacity => Some("Opacidad de rayos X"),
        TextKey::SettingsSolidColor => Some("Color sólido de la cabeza"),
        TextKey::SettingsLanguageGroup => Some("Idioma"),
        TextKey::SettingsMorphNames => Some("Nombres de morphs integrados"),
        TextKey::SettingsMorphNamesTranslated => Some("Traducidos"),
        TextKey::SettingsMorphNamesOriginal => Some("Original en inglés"),
        TextKey::SettingsLanguage => Some("Idioma de la interfaz"),
        TextKey::SettingsFoldersGroup => Some("Carpetas"),
        TextKey::SettingsVaMRoot => Some("Ruta de instalación de VaM"),
        TextKey::HairNeedsFinishedHead => Some("El cabello se lleva sobre una cabeza terminada."),
        TextKey::NoMatchingHair => Some("No hay preajustes de pelo coincidentes"),
        TextKey::MorphSearch => Some("Buscar morphs"),
        TextKey::MorphOneSidedFilter => Some("I/D"),
        TextKey::MorphOneSidedFilterShow => Some("Mostrar morphs que mueven un solo lado"),
        TextKey::MorphOneSidedFilterHide => Some("Ocultar morphs que mueven un solo lado"),
        TextKey::AppearanceSearch => Some("Buscar looks"),
        TextKey::AppearanceLayers => Some("Capas de aspecto"),
        TextKey::AddAppearanceLayer => Some("Anadir el aspecto seleccionado"),
        TextKey::AppearanceLayersEmpty => Some("Sin capas todavia"),
        TextKey::AppearanceLayerRaise => Some("Subir"),
        TextKey::AppearanceLayerLower => Some("Bajar"),
        TextKey::LookFind => Some("Cargar look"),
        TextKey::CameraTrackballArmed => {
            Some("Trackball · arrastra para girar libremente · clic o R para salir")
        }
        TextKey::HelpTrackball => Some("Rotación trackball"),
        TextKey::ShortcutTrackball => Some("R"),
        TextKey::SplitModelViewTooltip => Some("Verlo desde dos ángulos: dividir la vista"),
        TextKey::SplitModelViewName => Some("Vista dividida"),
        TextKey::HelpLevelRoll => Some("Nivelar el horizonte"),
        TextKey::ShortcutLevelRoll => Some("Alt+R"),
        TextKey::RecoveryTitle => Some("La última sesión terminó de forma inesperada"),
        TextKey::RecoveryBody => Some(
            "Tus morphs, el esculpido y las capas de textura se guardaron justo antes de que se detuviera. ¿Recuperarlos?",
        ),
        TextKey::RecoveryRestore => Some("Restaurar"),
        TextKey::RecoveryDiscard => Some("Empezar de cero"),
        TextKey::RecoveryRestored => Some("Se restauró la sesión anterior."),
        TextKey::RecoveryLookMissing => Some(
            "El look de esa sesión aún no está en la lista — las texturas se restauraron y las ediciones se aplicarán al seleccionarlo.",
        ),
        TextKey::SettingsBrushSweepGroup => Some("Gesto de tamaño de pincel"),
        TextKey::BrushSweepBlender => Some("Blender"),
        TextKey::BrushSweepZBrush => Some("ZBrush"),
        TextKey::BrushSweepTooltip => {
            Some("Blender confirma con un clic o una segunda pulsación; ZBrush al soltar la tecla")
        }
        TextKey::HairPackageStyle => Some("Empaquetar como .var"),
        TextKey::HairPackageDone => Some("Cabello empaquetado"),
        TextKey::HairPackageFailed => Some("Fallo al empaquetar"),
        TextKey::HairPackageNeedsInstall => Some("Instala el estilo primero"),
        TextKey::DialogOpenHeadPreset => Some("Abrir un preajuste de cabeza"),
        TextKey::FindHeadPreset => Some("Buscar un preajuste…"),
        TextKey::SettingsVignetteIntensity => Some("Cantidad"),
        TextKey::SettingsVignetteSmoothness => Some("Suavidad"),
        TextKey::SettingsVignetteRoundness => Some("Redondez"),
        TextKey::SculptNotCarried => Some("El esculpido no se pudo trasladar a este look"),
        TextKey::SettingsToneCurve => Some("Curva tonal"),
        TextKey::SettingsToneCurveTooltip => Some("Ajusta las sombras y las altas luces"),
        TextKey::SettingsVignetteTooltip => Some("Oscurece los bordes del encuadre"),
        TextKey::SettingsEffectEnabled => Some("Activado"),
        TextKey::SettingsInterfaceGroup => Some("Interfaz"),
        TextKey::SettingsTooltips => Some("Mostrar ayudas"),
        TextKey::SettingsResetGroup => Some("Restablecer"),
        TextKey::SettingsClearCache => Some("Vaciar la caché"),
        TextKey::SettingsClearCacheTooltip => Some(
            "Bases extraídas y bancos de morphs, que se rehacen cuando hacen falta. Vaciarlos solo cuesta un arranque más lento.",
        ),
        TextKey::SettingsResetAll => Some("Restablecer todos los ajustes y reiniciar"),
        TextKey::SettingsGraphicsLighting => Some("Iluminación"),
        TextKey::SettingsGraphicsEffects => Some("Efectos"),
        TextKey::SettingsGraphicsQuality => Some("Calidad"),
        TextKey::SettingsMsaa => Some("MSAA"),
        TextKey::SettingsMsaaTooltip => Some(
            "Suaviza los bordes escalonados según las muestras elegidas. Más alto es más limpio y más costoso.",
        ),
        TextKey::SoloViewHint => Some("Mostrar solo esta parte"),
        TextKey::SoloViewRestoreHint => Some("Restaurar la vista anterior"),
        TextKey::ToneCurveFilmic => Some("Fílmica"),
        TextKey::ToneCurveSoft => Some("Suave"),
        TextKey::LooksHiddenForOtherFigure => Some("Los looks de la otra figura están ocultos"),
        TextKey::LookBelongsToOtherFigure => Some("Este look está hecho para la otra figura"),
        TextKey::MorphEdit => Some("Editar morphs"),
        TextKey::VaMMorphPairImported => Some("Par de morphs de VaM importado"),
        TextKey::VaMMorphPairRejected => Some("No se puede importar el par de morphs de VaM"),
        TextKey::MorphCategoryAll => Some("Todos"),
        TextKey::MorphCategoryEyes => Some("Ojos"),
        TextKey::MorphCategoryBrows => Some("Cejas"),
        TextKey::MorphCategoryNose => Some("Nariz"),
        TextKey::MorphCategoryMouth => Some("Boca"),
        TextKey::MorphCategoryJaw => Some("Mandíbula"),
        TextKey::MorphCategoryCheeks => Some("Mejillas"),
        TextKey::MorphCategoryEars => Some("Orejas"),
        TextKey::MorphCategoryHead => Some("Cara"),
        TextKey::MorphCategoryExpression => Some("Expresión"),
        TextKey::NoMatchingMorphs => Some("No hay morphs coincidentes"),
        TextKey::ResetMorphs => Some("Restablecer todo"),
        TextKey::LightRotationGesture => Some("Shift+Arrastrar clic derecho"),
        TextKey::UpdateAvailable => Some("Actualizar"),
        TextKey::PackageFromFiles => Some("Elegir archivos"),
        TextKey::PackageMorphFiles => Some("Morphs"),
        TextKey::PackageTextureFiles => Some("Texturas"),
        TextKey::PackageListEmpty => Some("Aún no hay archivos"),
        TextKey::LightingStudio => Some("Estudio"),
        TextKey::LightingSoft => Some("Suave"),
        TextKey::LightingDaylight => Some("Luz de día"),
        TextKey::LightingWarm => Some("Cálido"),
        TextKey::LightingGloss => Some("Comprobar brillo"),
        TextKey::LightingPortrait => Some("Retrato"),
        TextKey::ImportLoadingMesh => Some("Cargando malla"),
        TextKey::ImportSimplifying => Some("Simplificando malla"),
        TextKey::StageOptimizeHead => Some("Optimizar cabeza"),
        TextKey::StagePrepareInput => Some("Preparar entrada"),
        TextKey::StageAlignScan => Some("Alinear escaneo"),
        TextKey::StageFitShape => Some("Ajustar forma G2"),
        TextKey::StageValidate => Some("Validar estructura"),
        TextKey::StagePrepareResult => Some("Preparar resultado"),
        TextKey::HeavyMeshNote => Some("unos {count} triangulos -- cuantos mas, mas tarda"),
        TextKey::TransformGroups => Some("Partes"),
        TextKey::CollapseTransformGroups => Some("Contraer partes"),
        TextKey::ExpandTransformGroups => Some("Expandir partes"),
        TextKey::Skin => Some("Piel"),
        TextKey::SculptTear => Some("Película lagrimal"),
        TextKey::SculptLips => Some("Labios"),
        TextKey::SculptTeethTongue => Some("Dientes · Lengua"),
        TextKey::SculptInnerMouth => Some("Interior de la boca"),
        TextKey::SculptEyes => Some("Ojos"),
        TextKey::Size => Some("Tamaño"),
        TextKey::SculptXSymmetry => Some("Simetría X"),
        TextKey::SculptXSymmetryTooltip => Some("Aplica el mismo trazo en ambos lados"),
        TextKey::BackfaceProtection => Some("Proteger reverso"),
        TextKey::AutoGroup => Some("Autogrupo"),
        TextKey::AutoGroupTooltip => Some("Deforma solo dentro de un grupo de polígonos"),
        TextKey::EyeTrackingAuto => Some("Auto"),
        TextKey::EyeGazeFreeze => Some("Fijar como morph"),
        TextKey::EyeGazeFreezeTooltip => Some(
            "Fija la mirada actual en los morphs, para que la pestaña Guardar exporte esta pose de ojos",
        ),
        TextKey::Reset => Some("Restablecer"),
        TextKey::SculptGrab => Some("Agarrar"),
        TextKey::SculptSmooth => Some("Suavizar"),
        TextKey::SculptPushPull => Some("Empujar · Tirar"),
        TextKey::ShortcutSculptGrab => Some("Arrastrar"),
        TextKey::ShortcutSculptSmooth => Some("Mayús + arrastrar"),
        TextKey::ShortcutSculptPushPull => Some("Ctrl / Alt + arrastrar"),
        TextKey::ResetSculpt => Some("Restablecer deformación"),
        #[cfg(test)]
        TextKey::SculptApplied => Some("Ediciones de esculpido aplicadas"),
        TextKey::SculptFailed => Some("Error en la operación de esculpido"),
        TextKey::Ready => Some("Listo"),
        TextKey::Busy => Some("La edición está bloqueada mientras se genera"),
        TextKey::NeedScan => Some(
            "Abre primero una cabeza escaneada OBJ, GLB o FBX, o parte de un look ya instalado en VaM",
        ),
        TextKey::ScanLoadFailed => Some("No se pudo leer ese archivo de cabeza"),
        TextKey::NeedEyeMorph => Some("El morph de ojos integrado no es compatible con este G2"),
        TextKey::ReviewEyePins => Some("Revisa los puntos rojos de los párpados"),
        TextKey::ResultUnavailable => Some("Ajusta primero la cara"),
        TextKey::MorphNotInLibrary => {
            Some("Ese morph aún no está en la biblioteca; el catálogo sigue cargando.")
        }
        TextKey::SculptNeedsBaseHead => {
            Some("Primero continúa con la cabeza base en la pestaña Crear.")
        }
        TextKey::MorphUnavailable => Some("No hay un morph de resultado compatible"),
        TextKey::AlignmentPending => {
            Some("Carga una cabeza escaneada y selecciona tu carpeta de VaM")
        }
        TextKey::ScanLoaded => Some("Cabeza personalizada OBJ, GLB o FBX cargada"),
        TextKey::ScanUnloaded => Some("Archivo de cabeza retirado; la base G2 está lista"),
        TextKey::PinPairsMismatched => Some("El número de pines no coincide; pines sin pareja"),
        TextKey::UnloadScanTooltip => Some("Quitar este archivo de cabeza"),
        TextKey::TemplateLoaded => Some("G2 cargado"),
        TextKey::PinsReset => Some("Se restablecieron todos los puntos"),
        TextKey::ResultStale => Some("Los cambios requieren regenerar el resultado"),
        TextKey::GenerationStarted => Some("Ajustando la cara"),
        TextKey::Cancelling => Some("Cancelando el ajuste de cara"),
        TextKey::GenerationComplete => Some("Ajuste de cara completado"),
        TextKey::GenerationCancelled => Some("Ajuste de cara cancelado"),
        TextKey::GenerationFailed => Some("Error en el ajuste de cara"),
        TextKey::ExportStarted => Some("Exportando"),
        TextKey::ExportComplete => Some("Exportación completada"),
        TextKey::ExportFailed => Some("Error al exportar"),
        TextKey::MorphNameHint => Some("Escribe un nombre de morph"),
        TextKey::MorphGroupHint => Some("Grupo  (p. ej. Custom, Morphs/Morph Loader)"),
        TextKey::MorphRegionHint => Some("Región  (p. ej. Custom, Morphs/Body)"),
        TextKey::DirectEntry => Some("Escribirlo"),
        TextKey::VaMMetadataPoseMorph => Some("Registrar como Pose Morph"),
        TextKey::VaMMetadataPoseMorphTooltip => {
            Some("Se gestiona junto con las expresiones y las poses")
        }
        TextKey::VaMMetadataBoneCorrection => Some("Corrección de esqueleto"),
        TextKey::VaMMetadataBoneCorrectionTooltip => {
            Some("Recentra los pivotes en el esqueleto remodelado")
        }
        TextKey::NativeCorePending => {
            Some("La integración del núcleo de ajuste nativo está en curso")
        }
        TextKey::MorphPreviewUpdated => Some("Vista previa del morph actualizada"),
        TextKey::MorphsReset => Some("Morphs restablecidos"),
        TextKey::MorphsResetUndone => Some("Restablecimiento de morphs deshecho"),
        TextKey::MorphEditUndone => Some("Ajuste del morph deshecho"),
        TextKey::MorphApplied => Some("Morph aplicado"),
        TextKey::HelpPlace => Some("Colocar punto"),
        TextKey::HelpMove => Some("Mover punto"),
        TextKey::HelpDelete => Some("Quitar punto"),
        TextKey::HelpXSymmetry => Some("Simetría X de puntos"),
        TextKey::HelpSnapRotation => Some("Rotación imantada"),
        TextKey::HelpDragZoom => Some("Zoom con arrastre"),
        TextKey::HelpOrbit => Some("Orbitar"),
        TextKey::HelpPan => Some("Desplazar"),
        TextKey::HelpZoom => Some("Zoom"),
        TextKey::HelpLight => Some("Rotar la luz"),
        TextKey::HelpFrameView => Some("Encuadrar cabeza"),
        TextKey::HelpUndo => Some("Deshacer"),
        TextKey::ShortcutPlace => Some("Clic izq."),
        TextKey::ShortcutMove => Some("Alt + arrastrar"),
        TextKey::ShortcutDelete => Some("Clic der."),
        TextKey::ShortcutXSymmetry => Some("X"),
        TextKey::ShortcutSnapRotation => Some("Mayús / Ctrl + arrastrar rotación"),
        TextKey::ShortcutDragZoom => Some("Ctrl + arrastrar vertical con botón central"),
        TextKey::ShortcutOrbit => Some("Arrastrar con botón central"),
        TextKey::ShortcutPan => Some("Mayús + arrastrar con botón central"),
        TextKey::ShortcutZoom => Some("Rueda del mouse"),
        TextKey::ShortcutLight => Some("Mayús + arrastrar der."),
        TextKey::ShortcutFrameView => Some("F"),
        TextKey::HelpSnapView => Some("Vista imantada"),
        TextKey::ShortcutSnapView => Some("Alt + arrastrar con la rueda"),
        TextKey::HelpStandardViews => Some("Vistas estándar"),
        TextKey::HelpCameraProjection => Some("Perspectiva / Ortográfica"),
        TextKey::ShortcutStandardViews => Some("Teclado num. 1-9"),
        TextKey::ShortcutCameraProjection => Some("Teclado numérico ."),
        TextKey::ShortcutUndo => Some("Ctrl+Z"),
        TextKey::HelpRedo => Some("Rehacer"),
        TextKey::ShortcutRedo => Some("Ctrl+Y"),
        TextKey::HairPresetSection => Some("Ajustes de pelo"),
        TextKey::HairPresetLoad => Some("Cargar"),
        TextKey::HairToolPick => Some("Elegir capa"),
        TextKey::HairToolPickHint => Some(
            "Haz clic en una mecha del viewport para activar su capa. El pincel espera al siguiente clic.",
        ),
        TextKey::HelpHairPick => Some("Elegir la capa bajo el cursor"),
        TextKey::ShortcutHairPick => Some("V, o Ctrl + clic con cualquier pincel"),
        TextKey::HelpHairSmooth => Some("Suavizar en vez de peinar"),
        TextKey::ShortcutHairSmooth => Some("Shift al peinar"),
        TextKey::HairGroupScalp => Some("Cuero cabelludo"),
        TextKey::HairScalpAlpha => Some("Recorte del cuero cabelludo"),
        TextKey::HairScalpPanel => Some("Cuero cabelludo"),
        TextKey::HairScalpMesh => Some("Malla del cuero cabelludo"),
        TextKey::HairScalpCreate => Some("Crear cuero cabelludo"),
        TextKey::HairScalpAbsent => Some("Este peinado aun no tiene capa de cuero cabelludo."),
        TextKey::HistoryBranchTitle => Some("Se perderán los pasos posteriores"),
        TextKey::HistoryBranchProceed => Some("Editar igualmente"),
        TextKey::DoNotShowAgain => Some("No volver a mostrar"),
        TextKey::SurfaceBaseUnblended => {
            Some("Algunos mapas PBR no pudieron mezclarse con la piel de VaM")
        }
        TextKey::DetailEditRedone => Some("Se rehízo la edición deshecha"),
        TextKey::ShortcutBrushSmaller => Some("Pincel más pequeño"),
        TextKey::ShortcutBrushLarger => Some("Pincel más grande"),
        TextKey::ShortcutBrushSizeDrag => Some("Tamaño del pincel"),
        TextKey::ShortcutBrushStrengthDrag => Some("Fuerza del pincel"),
        TextKey::ShortcutStencilCancel => Some("Cancelar la colocación del esténcil"),
        TextKey::ShortcutViewOrbit => Some("Orbitar la vista"),
        TextKey::ShortcutViewPan => Some("Desplazar la vista"),
        TextKey::ShortcutViewDolly => Some("Acercar y alejar la vista"),
        TextKey::TemplatePending => {
            Some("Selecciona tu carpeta de VaM para preparar la base G2 automáticamente")
        }
        TextKey::ResultEmpty => Some("No hay morph preparado"),
        TextKey::ScaleLink => Some("Vincular escala"),
        TextKey::ScaleLinkTooltip => Some("Vincula los valores de escala X/Y/Z"),
        TextKey::CameraSettings => Some("Ajustes de cámara"),
        TextKey::Perspective => Some("Perspectiva"),
        TextKey::Orthographic => Some("Ortográfica"),
        TextKey::Fov => Some("FOV"),
        TextKey::ResetCamera => Some("Restablecer cámara"),
        TextKey::WindowMinimize => Some("Minimizar"),
        TextKey::WindowMaximize => Some("Maximizar"),
        TextKey::WindowRestore => Some("Restaurar"),
        TextKey::WindowClose => Some("Cerrar"),
        TextKey::TooltipShow => Some("Mostrar"),
        TextKey::TooltipHide => Some("Ocultar"),
        TextKey::TooltipLock => Some("Bloquear"),
        TextKey::TooltipUnlock => Some("Desbloquear"),
        TextKey::AdjustEyeGaze => Some("Ajustar la mirada"),
        TextKey::ScanLoading => Some("Cargando cabeza escaneada"),
        TextKey::TemplateLoading => Some("Cargando plantilla G2"),
        TextKey::ResultLoading => Some("Cargando malla de resultado"),
        TextKey::VaMMorphPairLoading => Some("Cargando par de morphs de VaM"),
        TextKey::SystemLog => Some("Registro del sistema"),
        TextKey::Strength => Some("Fuerza"),
        TextKey::StrengthShort => Some("Fuerza"),
        TextKey::SculptBrushMove => Some("Agarrar"),
        TextKey::SculptBrushSmooth => Some("Suavizar"),
        TextKey::SculptBrushRestore => Some("Restaurar"),
        TextKey::SculptBrushMoveTooltip => {
            Some("Arrastra la superficie con el puntero; la fuerza indica cuánto la sigue")
        }
        TextKey::SculptBrushSmoothTooltip => Some("Suaviza el detalle de la superficie"),
        TextKey::SculptBrushRestoreTooltip => Some("Restaura hacia la forma base"),
        TextKey::SculptBrushMask => Some("Mascara"),
        TextKey::SculptBrushMaskTooltip => {
            Some("Recorta la capa de aspecto seleccionada para mostrar la de abajo. Alt la repinta")
        }
        TextKey::TextureLayers => Some("Capas de textura"),
        TextKey::PackageSection => Some("Exportar VAR"),
        TextKey::PackageCreatorHint => Some("Autor"),
        TextKey::PackageNameHint => Some("Nombre del paquete"),
        TextKey::PackageDescriptionHint => Some("Descripción"),
        TextKey::PackageCreditsHint => Some("Créditos"),
        TextKey::PackageInstructionsHint => Some("Instrucciones"),
        TextKey::PackageLinkHint => Some("Enlace de apoyo"),
        TextKey::PackageSave => Some("Guardar VAR"),
        TextKey::PackageFromThisHead => Some("Generar desde esta cabeza"),
        TextKey::PackageAddMorphs => Some("Añadir morphs"),
        TextKey::PackageAddTextures => Some("Añadir texturas"),
        TextKey::PackageNeedsVamRestart => Some("Hay que reiniciar VaM"),
        TextKey::PackageSavedTo => Some("Guardado en"),
        TextKey::FieldRequired => Some("(obligatorio)"),
        TextKey::FieldOptional => Some("(opcional)"),
        TextKey::PackageSaveTo => Some("Guardar en"),
        TextKey::PackageNothingToPack => {
            Some("No hay nada que empaquetar. Activa esta cabeza o añade un archivo.")
        }
        TextKey::PackageVersionMarker => Some("V"),
        TextKey::PackageCreatorRequired => Some("Se requiere un nombre de autor."),
        TextKey::PackageNameRequired => Some("Se requiere un nombre de paquete."),
        TextKey::PackageReplaceTitle => Some("Ya hay un VAR con este nombre"),
        TextKey::PackageReplaceBody => Some(
            "¿Reemplazarlo? Las escenas que referencian esta versión leerán el nuevo contenido.",
        ),
        TextKey::LicenseByTooltip => Some("acredita al autor"),
        TextKey::LicenseBySaTooltip => Some("atribuye al autor / comparte igual"),
        TextKey::LicenseByNdTooltip => Some("atribuye al autor / sin obras derivadas"),
        TextKey::LicenseByNcTooltip => Some("atribuye al autor / sin uso comercial"),
        TextKey::LicenseByNcSaTooltip => {
            Some("atribuye al autor / sin uso comercial / comparte igual")
        }
        TextKey::LicenseByNcNdTooltip => {
            Some("atribuye al autor / sin uso comercial / sin obras derivadas")
        }
        TextKey::LicensePcEaTooltip => Some("acceso anticipado para mecenas, sin redistribución"),
        TextKey::AddTextureLayer => Some("Añadir imagen"),
        TextKey::AddG2UvTextureLayer => Some("Añadir mapa UV"),
        TextKey::AddTextureLayerTooltip => {
            Some("Coloca una foto de rostro sobre la cabeza y la edita como textura")
        }
        TextKey::AddG2UvTextureLayerTooltip => {
            Some("Anade una textura face de G2 en su propio mapa UV y la edita")
        }
        TextKey::AddTextureLayerHint => {
            Some("Añade una imagen de cara o una textura G2 con el botón +.")
        }
        TextKey::BakingTextures => Some("Horneando texturas…"),
        TextKey::BakeTexturesForSave => Some("Hornear texturas para guardar"),
        TextKey::TextureMetalRough => Some("Metallic / Roughness"),
        TextKey::TextureGlossSmooth => Some("Glossiness / Smoothness"),
        TextKey::LayerOpacity => Some("Opacidad de la capa"),
        TextKey::PinOpacity => Some("Opacidad de los puntos"),
        TextKey::TransferPins => Some("Transferir puntos"),
        TextKey::TransferPinsTooltip => {
            Some("Copia los puntos que colocaste a las imágenes de las demás capas.")
        }
        TextKey::DeleteLayer => Some("Eliminar capa"),
        TextKey::TextureNormalStrength => Some("Fuerza del normal"),
        TextKey::TextureInvertScalar => Some("Invertir valores escalares"),
        TextKey::BlendNormal => Some("Normal"),
        TextKey::BlendMultiply => Some("Multiplicar"),
        TextKey::BlendScreen => Some("Trama"),
        TextKey::BlendOverlay => Some("Superponer"),
        TextKey::MatchToneToLayerBelow => Some("Igualar el tono a la capa inferior"),
        TextKey::ImageMirror => Some("Reflejar izq./der."),
        TextKey::ImageMirrorOff => Some("Ninguno"),
        TextKey::ImageMirrorToLeft => Some("A la izquierda"),
        TextKey::ImageMirrorToRight => Some("A la derecha"),
        TextKey::ImageAdjustments => Some("Ajustes de imagen"),
        TextKey::Exposure => Some("Exposición"),
        TextKey::Contrast => Some("Contraste"),
        TextKey::Saturation => Some("Saturación"),
        TextKey::Hue => Some("Tono"),
        TextKey::Temperature => Some("Temperatura"),
        TextKey::TextureMaskPreviewTooltip => {
            Some("Muestra la máscara de la capa activa como una superposición roja translúcida")
        }
        TextKey::ClearLayerMask => Some("Borrar la máscara de capa"),
        TextKey::ResetSourceRetouch => Some("Restablecer el retoque del origen"),
        TextKey::CloneSampleHint => {
            Some("Alt+clic en la imagen de origen para elegir la muestra de clonado.")
        }
        TextKey::BakeBase => Some("Opciones de horneado"),
        TextKey::TransparentOverlay => Some("Hornear solo las capas"),
        TextKey::CurrentSkinBake => Some("Hornear con la piel de VaM"),
        TextKey::TransparentOverlayTooltip => {
            Some("Hornea solo las imágenes de las capas, sin el preajuste de piel")
        }
        TextKey::CurrentSkinBakeTooltip => {
            Some("Hornea las capas sobre el preajuste de piel como base")
        }
        TextKey::SkinPresetRequired => Some("Elige primero un preajuste en Ajustes de piel"),
        TextKey::SkinSettings => Some("Ajustes de piel"),
        TextKey::ScanHeadColor => Some("Color de la cabeza escaneada"),
        TextKey::G2HeadColor => Some("Color de la cabeza G2"),
        TextKey::MorphFilterAll => Some("Todos"),
        TextKey::MorphFilterActive => Some("Activos"),
        TextKey::BrushDodge => Some("Sobreexponer"),
        TextKey::BrushBurn => Some("Subexponer"),
        TextKey::BrushSaturate => Some("Saturar"),
        TextKey::BrushDesaturate => Some("Desaturar"),
        TextKey::ShowLayersOnly => Some("Solo imágenes de capas"),
        TextKey::ShowLayersOnlyTooltip => {
            Some("Oculta la piel de VaM y muestra solo las imágenes de capas")
        }
        TextKey::MatchToneTooltip => {
            Some("Ajusta exposición, saturación y temperatura a la capa inferior")
        }
        TextKey::ClearLayerMaskTooltip => Some("Borra la máscara de esta capa"),
        TextKey::TextureMetalRoughTooltip => {
            Some("Guarda los mapas en la convención metallic/roughness")
        }
        TextKey::TextureGlossSmoothTooltip => {
            Some("Guarda los mapas en la convención glossiness/smoothness de VaM")
        }
        TextKey::TextureInvertScalarTooltip => Some(
            "Invierte los valores si la fuente usa la convención opuesta (roughness ↔ glossiness)",
        ),
        TextKey::BaseWithoutSkin => Some("Capa"),
        TextKey::BaseWithoutSkinTooltip => Some(
            "Las capas pintadas sobre el color sólido, sin el preajuste; la textura exportada no cambia",
        ),
        TextKey::SolidColorTooltip => Some("Solo el color sólido, sin textura"),
        TextKey::TextureSkinTooltip => Some("Las capas pintadas sobre el preajuste de piel de VaM"),
        TextKey::BoundaryFeather => Some("Suavizar borde"),
        TextKey::BoundaryFeatherTooltip => Some(
            "Solo difuso. Los mapas como normal y rugosidad no se pueden fundir con alfa, así que su borde sigue siendo duro",
        ),
        TextKey::Baking => Some("Horneando…"),
        TextKey::TextureToolPinPair => Some(
            "Par de puntos — coloca puntos coincidentes en la cara 3D y en la imagen de origen",
        ),
        TextKey::TextureToolMask => Some("Máscara — pinta para revelar; mantén Alt para ocultar"),
        TextKey::TextureToolClone => {
            Some("Tampón de clonar — Alt+clic en una muestra y luego pinta para copiarla")
        }
        TextKey::TextureToolDodgeBurn => {
            Some("Sobreexponer/subexponer — aclara; mantén Alt para oscurecer")
        }
        TextKey::TextureToolSponge => Some("Esponja — añade saturación; mantén Alt para quitarla"),
        TextKey::SelectTextureLayer => Some("Selecciona una capa de textura."),
        TextKey::LoadingTextureImage => Some("Cargando imagen…"),
        TextKey::ScanTextureAlignment => {
            Some("La textura del escaneo hereda la alineación 3D ajustada.")
        }
        TextKey::NoTextureImage => Some("Sin imagen."),
        TextKey::MorphSavedReloadFemale => {
            Some("Pulsa Reload Custom Morphs en la pestaña Female Morphs de VaM para actualizar.")
        }
        TextKey::MorphSavedReloadMale => {
            Some("Pulsa Reload Custom Morphs en la pestaña Male Morphs de VaM para actualizar.")
        }
        TextKey::BootStarting => Some("Preparando Vkit"),
        TextKey::BootFonts => Some("Cargando fuentes"),
        TextKey::BootGraphics => Some("Iniciando los gráficos"),
        TextKey::BootSettings => Some("Cargando la configuración"),
        TextKey::BootTemplate => Some("Cargando G2"),
        TextKey::BootWorkspace => Some("Preparando el espacio de trabajo"),
        TextKey::BootReady => Some("Abriendo"),
        TextKey::StartupFailedTitle => Some("Vkit no pudo iniciarse"),
        TextKey::StartupFailedGraphics => Some(
            "Vkit dibuja con DirectX 12 y los gráficos de este equipo no pudieron proporcionarlo. Actualiza el controlador de la tarjeta gráfica o ejecuta Vkit en un equipo con una GPU compatible con DirectX 12. Una sesión de escritorio remoto o una máquina virtual a menudo no tiene una pantalla DirectX 12 propia.",
        ),
        TextKey::StartupFailedOther => {
            Some("Vkit se detuvo durante el inicio y no pudo abrir su ventana.")
        }
        TextKey::StartupFailedLogHint => Some("Lo que falló se escribió en:"),
        TextKey::DialogOpenScan => Some("Abrir cabeza OBJ, GLB o FBX"),
        TextKey::DialogAddUvLayer => Some("Añadir capa de textura G2 UV"),
        TextKey::DialogAddTextureLayer => Some("Añadir capa de textura"),
        TextKey::DialogSaveMorphPair => Some("Guardar morph VaM VMI + VMB"),
        TextKey::DialogChooseVamFolder => Some("Elegir carpeta de Virt-A-Mate"),
        _ => None,
    }
}

pub(super) fn hair_label(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some("Cantidad de pelo"),
        "curveDensity" => Some("Densidad de curva"),
        "iterations" => Some("Iteraciones"),
        "simulationEnabled" => Some("Física"),
        "collisionEnabled" => Some("Colisión"),
        "weight" => Some("Peso"),
        "drag" => Some("Resistencia del aire"),
        "gravityMultiplier" => Some("Multiplicador de gravedad"),
        "collisionRadius" => Some("Radio de colisión"),
        "collisionRadiusRoot" => Some("Radio de colisión (raíz)"),
        "friction" => Some("Fricción"),
        "bendResistance" => Some("Resistencia a doblarse"),
        "rootRigidity" => Some("Rigidez en la raíz"),
        "mainRigidity" => Some("Rigidez media"),
        "tipRigidity" => Some("Rigidez en la punta"),
        "jointRigidity" => Some("Rigidez de juntas"),
        "rigidityRolloffPower" => Some("Caída de rigidez"),
        "cling" => Some("Cohesión de estilo"),
        "clingRolloff" => Some("Caída de cohesión"),
        "snap" => Some("Ajuste"),
        "usePaintedRigidity" => Some("Usar rigidez pintada"),
        "width" => Some("Grosor"),
        "length1" => Some("Longitud 1"),
        "length2" => Some("Longitud 2"),
        "length3" => Some("Longitud 3"),
        "maxSpread" => Some("Dispersión máxima"),
        "spreadRoot" => Some("Dispersión (raíz)"),
        "spreadMid" => Some("Dispersión (media)"),
        "spreadTip" => Some("Dispersión (punta)"),
        "spreadMidpoint" => Some("Punto medio de dispersión"),
        "spreadCurvePower" => Some("Curva de dispersión"),
        "curlScale" => Some("Tamaño del rizo"),
        "curlFrequency" => Some("Frecuencia del rizo"),
        "curlScaleRandomness" => Some("Aleatoriedad de tamaño"),
        "curlFrequencyRandomness" => Some("Aleatoriedad de frecuencia"),
        "curlRoot" => Some("Rizo (raíz)"),
        "curlMid" => Some("Rizo (medio)"),
        "curlTip" => Some("Rizo (punta)"),
        "curlMidpoint" => Some("Punto medio del rizo"),
        "curlCurvePower" => Some("Curva del rizo"),
        "curlX" => Some("Rizo X"),
        "curlY" => Some("Rizo Y"),
        "curlZ" => Some("Rizo Z"),
        "curlNormalAdjust" => Some("Ajuste de normal del rizo"),
        "curlAllowReverse" => Some("Permitir sentido inverso"),
        "curlAllowFlipAxis" => Some("Permitir eje invertido"),
        "Alpha Adjust" => Some("Opacidad del cuero"),
        "Diffuse Color" => Some("Color difuso del cuero"),
        "Gloss" => Some("Brillo del cuero"),
        "Specular Intensity" => Some("Especular del cuero"),
        "Global Illumination Filter" => Some("Iluminación global del cuero cabelludo"),
        "Diffuse Texture Offset" => Some("Desplazamiento de textura del cuero"),
        "rootColor" => Some("Color de raíz"),
        "tipColor" => Some("Color de punta"),
        "specularColor" => Some("Color especular"),
        "colorRolloff" => Some("Caída de color raíz→punta"),
        "randomColorPower" => Some("Fuerza de color aleatorio"),
        "randomColorOffset" => Some("Desplazamiento de color aleatorio"),
        "primarySpecularSharpness" => Some("Brillo primario"),
        "secondarySpecularSharpness" => Some("Brillo secundario"),
        "specularShift" => Some("Desplazamiento del brillo secundario"),
        "diffuseSoftness" => Some("Suavidad difusa"),
        "fresnelPower" => Some("Fuerza de Fresnel"),
        "fresnelAttenuation" => Some("Atenuación de Fresnel"),
        "IBLFactor" => Some("Luz indirecta"),
        "normalRandomize" => Some("Aleatoriedad de normales"),
        _ => None,
    }
}

pub(super) fn hair_hint(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some(
            "Cuántos mechones crecen entre cada trío de guías. Es el aspecto del pelo, y la mayor parte de su coste.",
        ),
        "curveDensity" => Some(
            "Puntos dibujados a lo largo de cada mechón. Más es más suave y más lento; la forma se lee mucho antes de que el número sea alto.",
        ),
        "iterations" => Some(
            "Pasadas del solver por fotograma. Más pasadas sujetan las restricciones con más firmeza en movimientos rápidos.",
        ),
        "simulationEnabled" => Some(
            "Si esta parte se mueve en el juego. No tiene relación con el interruptor del visor.",
        ),
        "collisionEnabled" => Some("Si los mechones de esta parte son empujados fuera del cuerpo."),
        "weight" => {
            Some("Con cuánto peso cae un mechón. Escala la gravedad y su resistencia a ser movido.")
        }
        "drag" => Some(
            "Cuánto retiene el aire a un mechón. Con valores altos se asienta pronto y oscila poco.",
        ),
        "gravityMultiplier" => {
            Some("La gravedad sobre esta parte, como múltiplo de la de la escena.")
        }
        "collisionRadius" => {
            Some("El grosor con el que un mechón colisiona, a lo largo de toda su longitud.")
        }
        "collisionRadiusRoot" => Some("El grosor cerca de la raíz, cuando deba diferir del resto."),
        "friction" => Some("Cuánto se agarra un mechón a lo que toca en lugar de deslizarse."),
        "bendResistance" => Some("Cuánto resiste un mechón a ser doblado desde la línea recta."),
        "rootRigidity" => Some("Con cuánta fuerza la raíz mantiene la forma peinada."),
        "mainRigidity" => Some("Con cuánta fuerza la parte media mantiene la forma peinada."),
        "tipRigidity" => Some("Con cuánta fuerza la punta mantiene la forma peinada."),
        "jointRigidity" => Some(
            "La rigidez de las juntas de estilo: los muelles que mantienen una coleta como coleta.",
        ),
        "rigidityRolloffPower" => Some(
            "Cómo se mezclan las tres rigideces a lo largo del mechón. Más alto lleva el valor de la raíz más abajo.",
        ),
        "cling" => Some("Con cuánta fuerza las juntas de estilo atraen los mechones entre sí."),
        "clingRolloff" => Some(
            "Hasta dónde se desvanece la cohesión a lo largo del mechón. Más alto la confina cerca de la raíz.",
        ),
        "snap" => {
            Some("Con cuánta firmeza cada segmento mantiene la longitud con la que fue creado.")
        }
        "usePaintedRigidity" => {
            Some("Tomar la rigidez del mapa pintado en lugar de los tres deslizadores.")
        }
        "width" => Some(
            "El grosor con el que se dibuja un mechón. El deslizador de VaM también termina en una décima de milímetro.",
        ),
        "length1" => {
            Some("Longitud de los mechones más cercanos a la primera guía de su triángulo.")
        }
        "length2" => {
            Some("Longitud de los mechones más cercanos a la segunda guía de su triángulo.")
        }
        "length3" => {
            Some("Longitud de los mechones más cercanos a la tercera guía de su triángulo.")
        }
        "maxSpread" => Some(
            "Un techo para lo lejos que un mechón puede alejarse de su guía, no una ganancia. Una vez supera la separación entre guías, subirlo no hace nada.",
        ),
        "spreadRoot" => Some(
            "Con cuánta libertad se separan los mechones en la raíz. Bajo los agrupa hacia la guía.",
        ),
        "spreadMid" => Some("Con cuánta libertad se separan los mechones en la parte media."),
        "spreadTip" => Some("Con cuánta libertad se separan los mechones en la punta."),
        "spreadMidpoint" => Some("Desde qué punto del mechón se aplica la dispersión media."),
        "spreadCurvePower" => {
            Some("Con cuánta brusquedad gira la dispersión entre raíz, media y punta.")
        }
        "curlScale" => Some("El ancho del rizo: el radio sobre el que se enrolla, no su longitud."),
        "curlFrequency" => Some("Vueltas por centímetro de mechón."),
        "curlScaleRandomness" => Some("Cuánto varía el ancho del rizo de un mechón a otro."),
        "curlFrequencyRandomness" => Some("Cuánto varía el ritmo de vuelta de un mechón a otro."),
        "curlRoot" => Some("Cuánto rizo alcanza la raíz."),
        "curlMid" => Some("Cuánto rizo alcanza la parte media."),
        "curlTip" => Some("Cuánto rizo alcanza la punta."),
        "curlMidpoint" => Some("Desde qué punto del mechón se aplica el rizo medio."),
        "curlCurvePower" => Some("Con cuánta brusquedad gira el rizo entre raíz, media y punta."),
        "curlX" => Some(
            "El desplazamiento del rizo a lo largo de X. Un vector que se rota, no un eje sobre el que enrollar.",
        ),
        "curlY" => Some("El desplazamiento del rizo a lo largo de Y."),
        "curlZ" => Some("El desplazamiento del rizo a lo largo de Z."),
        "curlNormalAdjust" => Some(
            "Se suma al desplazamiento del rizo a lo largo de la normal. Levanta la espiral de la cabeza en lugar de inclinarla.",
        ),
        "curlAllowReverse" => Some(
            "Deja que algunos mechones se enrollen al revés, para que una melena rizada no se lea como un mechón repetido.",
        ),
        "curlAllowFlipAxis" => Some(
            "Deja que algunos mechones se enrollen sobre un eje invertido, por la misma razón.",
        ),
        "Alpha Adjust" => Some(
            "Lo opaco que es el casquete del cuero cabelludo. La mayoría de las partes llevan el casquete de otro y lo dejan al mínimo para ocultarlo; súbelo solo cuando una raya deba mostrar cuero.",
        ),
        "Diffuse Color" => Some(
            "El color difuso del propio casquete. VaM lo distribuye en blanco, para que la piel de debajo se transparente.",
        ),
        "Gloss" => Some("Lo brillante que es el casquete allí donde se ve."),
        "Specular Intensity" => Some("Con cuánta fuerza el casquete recoge un brillo."),
        "Global Illumination Filter" => Some("Cuánta luz ambiental recoge la capa."),
        "Diffuse Texture Offset" => {
            Some("Se suma al color difuso del casquete. Pese al nombre, no es un índice de hoja.")
        }
        "rootColor" => Some("El color en la raíz."),
        "tipColor" => Some("El color en la punta."),
        "specularColor" => Some("El color del brillo."),
        "colorRolloff" => {
            Some("Cómo cede el color de la raíz al de la punta a lo largo del mechón.")
        }
        "randomColorPower" => Some("Con cuánta fuerza varían los mechones de color entre sí."),
        "randomColorOffset" => {
            Some("Hacia dónde se inclina esa variación, más clara o más oscura.")
        }
        "primarySpecularSharpness" => {
            Some("Lo cerrado que es el brillo principal. Más alto es más lustroso.")
        }
        "secondarySpecularSharpness" => {
            Some("Lo cerrado que es el segundo brillo: el coloreado que viaja con el pelo.")
        }
        "specularShift" => {
            Some("A qué distancia del primero se sitúa el segundo brillo a lo largo del mechón.")
        }
        "diffuseSoftness" => Some(
            "Con cuánta suavidad envuelve la luz un mechón en lugar de iluminar solo el lado que da a ella.",
        ),
        "fresnelPower" => {
            Some("Con cuánta brusquedad se ilumina el borde donde el pelo se aparta de la vista.")
        }
        "fresnelAttenuation" => Some("La fuerza de esa iluminación del borde."),
        "IBLFactor" => Some("Cuánta luz ambiental de la escena recoge el pelo."),
        "normalRandomize" => Some(
            "Cuánto se altera la normal de sombreado de cada mechón, para que los vecinos no se sombreen igual.",
        ),
        _ => None,
    }
}
