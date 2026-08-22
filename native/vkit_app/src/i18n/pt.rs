use super::TextKey;

pub(super) const fn label(key: TextKey) -> Option<&'static str> {
    #[allow(unreachable_patterns, reason = "the fallback outlives today's key set")]
    match key {
        TextKey::SurfaceSmooth => Some("Suavização da malha"),
        TextKey::SurfaceSmoothTooltip => Some(
            "Suavização de render compatível com VaM. 0 é a malha base editável, 3 é o padrão do VaM e 4 é o máximo. Morphs, UVs, escultura e bake de texturas mantêm a topologia base",
        ),
        TextKey::FaceMatch => Some("Criar"),
        TextKey::ResultPreview => Some("Salvar"),
        TextKey::Save => Some("Salvar"),
        TextKey::DetailCorrection => Some("Esculpir"),
        TextKey::TextureStage => Some("Textura"),
        TextKey::HairStage => Some("Cabelo"),
        TextKey::HairUnavailable => {
            Some("Conclua o resultado de escultura antes de editar o cabelo")
        }
        TextKey::HairPartsPanel => Some("Partes de cabelo"),
        TextKey::AddHairPart => Some("Adicionar parte de cabelo"),
        TextKey::HairScalpMeshCrowded => {
            Some("Algumas mechas não couberam na nova malha de couro cabeludo e foram descartadas")
        }
        TextKey::HairScalpMissing => {
            Some("Malha do couro cabeludo ausente; abra uma raiz do VaM e reescaneie")
        }
        TextKey::HairShowPoints => Some("Mostrar pontos"),
        TextKey::HairShowPointsHint => Some(
            "Mostra os pontos de plantio do couro cabeludo. Desligue para ver so o cabelo; os pinceis continuam funcionando.",
        ),
        TextKey::HairPartTint => Some("Tingir partes"),
        TextKey::HairPartTintHint => Some(
            "Dá a cada parte um tom distinto no visor para diferenciá-las. Apenas visual; não afeta a exportação.",
        ),
        TextKey::HairSettingsPanel => Some("Ajustes de cabelo"),
        TextKey::HairCreateFirst => Some("Crie algum cabelo primeiro."),
        TextKey::HairHideStrands => Some("Ocultar cabelo"),
        TextKey::HairHideStrandsHint => Some(
            "Remove o cabelo gerado para ler so as mechas guia. Ligar isto liga tambem as mechas.",
        ),
        TextKey::HairShowStreams => Some("Ver mechas guia"),
        TextKey::HairShowStreamsHint => {
            Some("Desenha a mecha que voce edita, sobre o cabelo que ela gera.")
        }
        TextKey::HairViewportPhysics => Some("Física da viewport"),
        TextKey::HairViewportPhysicsHint => {
            Some("Pre-visualiza a fisica na tela. Nao tem relacao com a exportacao.")
        }
        TextKey::HairMirrorEdit => Some("Edição espelhada"),
        TextKey::HairMirrorEditHint => {
            Some("Os pincéis editam os dois lados ao mesmo tempo, em simetria.")
        }
        TextKey::HairAutoPart => Some("Parte automática"),
        TextKey::HairAutoPartHint => Some(
            "O pincel edita apenas a parte reconhecida sob o cursor, em vez das camadas ativas.",
        ),
        TextKey::HairExportSection => Some("Salvar cabelo"),
        TextKey::HairExportBothSexes => Some("Ambos"),
        TextKey::HairExportRescanNotice => {
            Some("Clique em VaM - Hair - Rescan Files para vê-lo na lista")
        }
        TextKey::HairExportOverwroteNotice => {
            Some("Uma miniatura sobrescrita aparece após Hard Reset ou reiniciar o VaM")
        }
        TextKey::HairSimToggleHint => Some("Executa a fisica do cabelo aqui e no jogo."),
        TextKey::HairCollisionToggleHint => {
            Some("Impede que o cabelo atravesse a cabeca e o corpo.")
        }
        TextKey::HairStyleJoints => Some("Juntas de estilo"),
        TextKey::HairStyleJointsHint => Some(
            "Amarra mechas próximas para o penteado manter a forma no jogo; é o que o Cling escala. Para cabelo longo ou preso; aumenta o arquivo.",
        ),
        TextKey::HairThumbnailPrompt => Some("Enquadre o ângulo e clique para capturar"),
        TextKey::HairThumbnailShoot => Some("Capturar"),
        TextKey::HairThumbnailSkip => Some("Pular"),
        TextKey::HairThumbnailSaved => Some("Miniatura salva"),
        TextKey::HairThumbnailFailed => Some("Falha ao salvar a miniatura"),
        TextKey::HairGameOnly => Some(
            "Ajuste de física: o visor não simula, então só age dentro do VaM. Exportado com o estilo.",
        ),
        TextKey::HairPartVisible => Some("Mostrar ou ocultar esta parte de cabelo"),
        TextKey::RemoveHairPart => Some("Remover parte de cabelo"),
        TextKey::HairRenamePart => Some("Clique para renomear"),
        TextKey::HairToolPlant => Some("Plantar"),
        TextKey::HairToolErase => Some("Apagar"),
        TextKey::HairToolGrow => Some("Crescer (Alt encurta)"),
        TextKey::HairToolComb => Some("Pentear"),
        TextKey::HairToolPlantHint => {
            Some("Planta fios nos vertices do couro cabeludo sob o pincel")
        }
        TextKey::HairToolGrowHint => Some("Alonga os fios sob o pincel; segure Alt para encurtar"),
        TextKey::HairToolEraseHint => Some("Remove os fios sob o pincel"),
        TextKey::HairToolCombHint => Some("Arraste os fios sob o pincel; as raizes ficam paradas"),
        TextKey::HairToolPinch => Some("Agrupar"),
        TextKey::HairToolPinchHint => Some("Junta as mechas sob o pincel. Alt as separa"),
        TextKey::HairToolCut => Some("Cortar"),
        TextKey::HairToolCutHint => Some("Corta as mechas onde o pincel as atravessa"),
        TextKey::HairToolPuff => Some("Volume"),
        TextKey::HairToolPuffHint => Some("Levanta as mechas sob o pincel. Alt as achata"),
        TextKey::HairToolVertex => Some("Ponto"),
        TextKey::HairToolVertexHint => {
            Some("Segure uma junta de um fio e mova-a; o comprimento é mantido")
        }
        TextKey::HairCopySettings => Some("Copiar tudo"),
        TextKey::HairPasteSettings => Some("Colar tudo"),
        TextKey::HairGroupPerformance => Some("Desempenho"),
        TextKey::HairGroupPhysics => Some("Fisica"),
        TextKey::HairGroupStiffness => Some("Rigidez"),
        TextKey::HairGroupShape => Some("Forma"),
        TextKey::HairGroupCurl => Some("Cacho"),
        TextKey::HairGroupLook => Some("Aparencia"),
        TextKey::HairColorScalp => Some("Couro"),
        TextKey::HairColorRoot => Some("Raiz"),
        TextKey::HairColorTip => Some("Ponta"),
        TextKey::HairColorSpecular => Some("Brilho"),
        TextKey::HairScalpMask => Some("Textura do couro"),
        TextKey::HairScalpMaskBuiltIn => Some("Integrada"),
        TextKey::HairScalpMaskCustom => Some("De um arquivo..."),
        TextKey::HairExport => Some("Exportar para VaM"),
        TextKey::HairExportName => Some("Nome do item"),
        TextKey::HairExportCreator => Some("Criador"),
        TextKey::HairExportNeedsMetadata => Some("Informe o nome e o criador antes de exportar"),
        TextKey::HairOverwriteTitle => Some("Substituir este estilo?"),
        TextKey::HairOverwriteBody => {
            Some("Ja existe um estilo com este nome e criador. Exportar substitui os arquivos.")
        }
        TextKey::HairOverwriteProceed => Some("Substituir"),
        TextKey::HairExportDone => Some("Item de cabelo salvo"),
        TextKey::HairExportFailed => Some("Falha ao exportar cabelo"),
        TextKey::HairExportNeedsPart => Some("Selecione primeiro uma parte com mechas"),
        TextKey::HairExportNeedsVaMRoot => Some("Defina uma raiz do VaM antes de exportar"),
        TextKey::HairMirrorPart => Some("Espelhar a parte para o outro lado"),
        TextKey::HairDuplicatePart => Some("Duplicar parte"),
        TextKey::HairSegmentsShort => Some("Seg"),
        TextKey::MorphCategoryBody => Some("Corpo"),
        TextKey::TextureNamePlaceholder => Some("Digite um nome de textura"),
        TextKey::TextureOverwriteTitle => Some("Isto substitui as texturas com o mesmo nome"),
        TextKey::OpenScan => Some("Scan"),
        TextKey::SourceMorphMissing => Some("Nenhum morph deste look está instalado"),
        TextKey::EditDetails => Some("Editar detalhes"),
        TextKey::Female => Some("Mulher"),
        TextKey::Male => Some("Homem"),
        TextKey::Lighting => Some("Iluminação"),
        TextKey::Brightness => Some("Brilho"),
        TextKey::LightRotation => Some("Rotação da luz"),
        TextKey::AddHeadFileHint => Some("ou arraste um arquivo OBJ, GLB ou FBX aqui"),
        TextKey::AddHeadFile => Some("Adicionar um arquivo de cabeça"),
        TextKey::AutoAlign => Some("Alinhamento automático"),
        TextKey::PlacementReset => Some("Redefinir posição"),
        TextKey::Scale => Some("Escala"),
        TextKey::Position => Some("Posição"),
        TextKey::Rotation => Some("Rotação"),
        TextKey::XMirror => Some("Espelho X"),
        TextKey::XMirrorTooltip => {
            Some("Ao colocar um ponto, o gêmeo também é colocado do outro lado")
        }
        TextKey::NumbersTooltip => Some("Numera cada ponto na ordem em que foi colocado"),
        TextKey::XMirrorRejected => {
            Some("O espelho X foi desativado porque não há contraparte segura")
        }
        TextKey::ScanFidelity => Some("Fidelidade ao scan"),
        TextKey::ScanOptions => Some("Opções de escaneamento"),
        TextKey::RestoreNeckEars => Some("Restaurar pescoço e orelhas"),
        TextKey::RestoreNeckEarsTooltip => Some(
            "Mantém orelhas, nuca e parte de trás do crânio na base G2, mesclando o ajuste apenas em direção ao rosto",
        ),
        TextKey::RestoreNeckEarsDeferred => {
            Some("Há traços de escultura; a mudança se aplicará na próxima geração")
        }
        TextKey::ScanFidelityTooltip => Some(
            "Acima de 1 o ajuste segue mais a forma do scan e preserva menos a suavidade da figura. Até 2 é só isso; passando de 2, o ruído do scan — pontinhos, furos, emendas de costura — também começa a aparecer como saliências, o que pode valer a pena em um scan limpo e denso. Os pontos nunca são sacrificados em nenhum valor e nenhuma proteção é afrouxada.",
        ),
        TextKey::ScanSymmetry => Some("Simetria do scan"),
        TextKey::Original => Some("Original"),
        TextKey::Left => Some("Esquerda"),
        TextKey::Right => Some("Direita"),
        TextKey::EyesOpen => Some("Olhos abertos"),
        TextKey::EyesClosed => Some("Olhos fechados"),
        TextKey::View => Some("Opções de vista"),
        TextKey::On => Some("Ativo"),
        TextKey::Numbers => Some("Números"),
        TextKey::PinPairsPlaced => Some("Pontos"),
        TextKey::PinPlacement => Some("Posicionamento dos pontos"),
        TextKey::ScanReference => Some("Referência do scan"),
        TextKey::AddHeadFileAction => Some("Adicionar um arquivo de cabeça"),
        TextKey::AddHeadFileActionTooltip => {
            Some("Carrega um arquivo de cabeça escaneada (OBJ · GLB · FBX)")
        }
        TextKey::ContinueStep => Some("Continuar com a cabeça base"),
        TextKey::ContinueStepTooltip => Some(
            "Edita a cabeça G2 como está, sem scan; um scan ainda pode ser carregado e ajustado depois",
        ),
        TextKey::Undo => Some("Desfazer"),
        TextKey::ResetAllPins => Some("Redefinir todos os pontos"),
        TextKey::ResetAll => Some("Redefinir tudo"),
        TextKey::MorphSave => Some("Salvar morph"),
        TextKey::TextureSaveSection => Some("Salvar texturas"),
        TextKey::Generate => Some("Ajustar rosto"),
        TextKey::Cancel => Some("Cancelar"),
        TextKey::CopyAll => Some("Copiar tudo"),
        TextKey::Next => Some("Salvar"),
        TextKey::TextureSkin => Some("Preset"),
        TextKey::SolidColor => Some("Sólido"),
        TextKey::BackfaceProtectionTooltip => Some("Protege o verso das malhas finas"),
        TextKey::WireframeColor => Some("Cor do aramado"),
        TextKey::WireframeSettings => Some("Aramado"),
        TextKey::XraySettings => Some("Raio-X"),
        TextKey::ViewportLightingTooltip => Some("Ajusta a iluminação"),
        TextKey::Background => Some("Fundo"),
        TextKey::ViewportBackgroundTooltip => Some("Ajusta o fundo e a imagem de referência"),
        TextKey::BackgroundRadial => Some("Radial"),
        TextKey::BackgroundVertical => Some("Vertical"),
        TextKey::BackgroundFlat => Some("Plano"),
        TextKey::ViewportWireframeTooltip => Some("Mostra e ajusta a sobreposição de aramado"),
        TextKey::ViewportXrayTooltip => Some("Mostra e ajusta a sobreposição de raio-X"),
        TextKey::ViewportSkinTooltip => Some("Escolha uma textura de pele do VaM"),
        TextKey::FalloffTooltip => Some("Escolha a curva de influência do pincel"),
        TextKey::FalloffSmooth => Some("Suave"),
        TextKey::FalloffSmoother => Some("Mais suave"),
        TextKey::FalloffSharp => Some("Acentuado"),
        TextKey::FalloffLinear => Some("Linear"),
        TextKey::PlaceHead => Some("Posicionar a cabeça"),
        TextKey::PlaceHeadTooltip => {
            Some("Sobrepõe a cabeça carregada ao G2 para alinhar posição, rotação e tamanho.")
        }
        TextKey::ScanOverlay => Some("Sobrepor scan"),
        TextKey::OverlayDone => Some("Concluir sobreposição"),
        TextKey::ProjectImage => Some("Projetar"),
        TextKey::ProjectionCanvasHint => {
            Some("Pinte na vista 3D e o traço aparece neste canvas UV")
        }
        TextKey::HelpProjectionPaint => Some("Pintar (projetar)"),
        TextKey::ShortcutProjectionPaint => Some("Arrastar esq."),
        TextKey::HelpProjectionMove => Some("Mover imagem"),
        TextKey::ShortcutProjectionMove => Some("Arrastar dir."),
        TextKey::HelpProjectionRotate => Some("Girar imagem"),
        TextKey::ShortcutProjectionRotate => Some("Shift + arrastar dir."),
        TextKey::HelpProjectionZoom => Some("Escalar imagem"),
        TextKey::ShortcutProjectionZoom => Some("Roda"),
        TextKey::HelpBrushSize => Some("Tamanho do pincel"),
        TextKey::ShortcutBrushSize => Some("Arrastar F · [ ]"),
        TextKey::HelpBrushStrength => Some("Intensidade do pincel"),
        TextKey::ShortcutBrushStrength => Some("Shift+F arrastar"),
        TextKey::HelpProjectionDone => Some("Concluir projeção"),
        TextKey::ShortcutProjectionDone => Some("Concluir · Esc"),
        TextKey::ProjectDone => Some("Concluir"),
        TextKey::ProjectDoneTooltip => {
            Some("Conclui a projeção; tudo o que foi pintado permanece na camada")
        }
        TextKey::MorphCompare => Some("Comparar"),
        TextKey::Opacity => Some("Opacidade"),
        TextKey::G2Head => Some("Cabeça G2"),
        TextKey::Eyelashes => Some("Cílios"),
        TextKey::OverwriteTitle => Some("Substituir arquivo"),
        TextKey::OverwriteBody => Some("Já existe um arquivo com o mesmo nome. Substituir?"),
        TextKey::Overwrite => Some("Substituir"),
        TextKey::ScanTextureLayer => Some("Textura do scan"),
        TextKey::PinPrompt => Some("Coloque um pino em cada um, ou ajuste sem eles"),
        TextKey::TextureNeedsImage => Some("Adicione uma imagem pelo painel lateral"),
        TextKey::TexturePinPairPrompt => {
            Some("Coloque pinos correspondentes à esquerda e à direita")
        }
        TextKey::TextureToolNeedsPins => Some("Coloque os pontos primeiro para usar"),
        TextKey::SymmetrySuggestion => {
            Some("Recomenda-se o espelhamento para um ajuste mais limpo")
        }
        TextKey::SymmetryChangeTitle => Some("Os pontos serão redefinidos"),
        TextKey::SymmetryChangeConfirm => Some("Confirmar"),
        TextKey::EyeClosure => Some("Fechar olhos"),
        TextKey::VaMFolder => Some("Pasta do VaM"),
        #[cfg(test)]
        TextKey::VaMGeometryBaseReady => Some("Base de geometria do VaM verificada"),
        TextKey::VaMGeometryBaseRejected => Some("Não é possível usar a base de geometria do VaM"),
        TextKey::VaMGeometryBaseHowTo => Some(
            "Selecione sua pasta do VaM; a base neutra G2 é extraída automaticamente da sua instalação",
        ),
        TextKey::VaMBaseSourceUnityBundle => {
            Some("Base neutra nativa do VaM (extraída automaticamente)")
        }
        TextKey::VaMBaseSourceDazAnchorPair => Some("Âncora DAZ + base OBJ verificada"),
        TextKey::VaMBaseSourceUnityAnchorPair => {
            Some("Âncora neutra extraída + base OBJ verificada")
        }
        TextKey::VaMBaseSourceUnverifiedObj => Some("Base OBJ não verificada"),
        TextKey::VaMUvMappingUnavailable => Some(
            "O mapeamento UV da pele do VaM não está pronto. Verifique sua pasta do VaM e atualize o catálogo de peles",
        ),
        TextKey::VaMIndexing => Some("Indexando o catálogo do VaM"),
        TextKey::VaMFailed => Some("Falha no catálogo do VaM"),
        TextKey::RefreshSkins => Some("Atualizar biblioteca de peles"),
        TextKey::MorphLoading => Some("Carregando morph"),
        TextKey::MorphLoadFailed => Some("Falha ao carregar o morph"),
        TextKey::DefaultSkinMissing => {
            Some("Não foi possível encontrar essa pele. Verifique a pasta do VaM")
        }
        TextKey::DefaultSkinSet => Some("Abrir com esta pele"),
        TextKey::DefaultSkinClear => Some("Parar de abrir com esta pele"),
        TextKey::SkinNone => Some("Nenhuma"),
        TextKey::SkinSearch => Some("Buscar peles"),
        TextKey::SkinLoading => Some("Carregando pele"),
        TextKey::SkinReady => Some("Prévia da pele pronta"),
        TextKey::SkinLoadFailed => Some("Falha ao carregar a pele"),
        TextKey::SkinUvUnavailable => Some("UV da pele indisponível"),
        TextKey::NoMatchingSkins => Some("Nenhuma pele correspondente"),
        TextKey::HairSearch => Some("Buscar cabelo"),
        TextKey::HairReady => Some("Prévia do cabelo pronta"),
        TextKey::HairLoadFailed => Some("Falha ao carregar o cabelo"),
        TextKey::HairPartsSkipped => Some("Exibindo sem algumas partes"),
        TextKey::HairParts => Some("partes"),
        TextKey::BaseFace => Some("Rosto base"),
        TextKey::Settings => Some("Configurações"),
        TextKey::SettingsGraphics => Some("Gráficos"),
        TextKey::SettingsViewport => Some("Viewport"),
        TextKey::SettingsGeneral => Some("Geral"),
        TextKey::SettingsAbout => Some("Sobre"),
        TextKey::SettingsShortcuts => Some("Atalhos"),
        TextKey::ShortcutsCapturing => Some("Pressione…"),
        TextKey::ShortcutsResetAll => Some("Redefinir todos os atalhos"),
        TextKey::ShortcutsExport => Some("Salvar o mapa em um arquivo"),
        TextKey::ShortcutsImport => Some("Carregar um arquivo de mapa"),
        TextKey::ShortcutsTaken => Some("Outra ação já responde a isso"),
        TextKey::SettingsAboutBuild => Some("Compilação"),
        TextKey::SettingsAboutLicense => Some("Licença"),
        TextKey::SettingsAboutDiagnostics => Some("Diagnóstico"),
        TextKey::SettingsTooltipsTooltip => {
            Some("Ajuda como esta, que é o que acabou de acontecer")
        }
        TextKey::AboutTagline => {
            Some("Uma ferramenta complementar para criar morphs de cabeça do VaM.")
        }
        TextKey::AboutNoAffiliation => Some(
            "Virt-A-Mate pertence à MeshedVR e Genesis 2 à DAZ 3D. O Vkit é uma ferramenta independente, sem vínculo com nenhuma delas.",
        ),
        TextKey::AboutNoWarranty => Some(
            "Fornecido como está, sem garantia. O que você criar e a licença do que carregar são de sua responsabilidade.",
        ),
        TextKey::AboutSupport => Some("Apoio"),
        TextKey::AboutReportProblem => Some(
            "O Vkit grava um log a cada execução e um arquivo à parte se cair. Abra a pasta e anexe os dois ao relato: uma falha que ninguém mais consegue ver é uma falha que ninguém consegue corrigir.",
        ),
        TextKey::AboutOpenLogs => Some("Abrir a pasta de logs"),
        TextKey::AboutReportIssue => Some("Relatar um problema"),
        TextKey::SettingsLightingGroup => Some("Iluminação"),
        TextKey::SettingsLightingPreset => Some("Preset"),
        TextKey::SettingsExposure => Some("Exposição"),
        TextKey::SettingsGeometryGroup => Some("Geometria"),
        TextKey::SettingsSmoothPasses => Some("Suavização da superfície"),
        TextKey::SettingsSmoothPassesHint => {
            Some("Suaviza apenas a exibição, como o VaM faz. A forma salva não muda.")
        }
        TextKey::SettingsBackgroundGroup => Some("Fundo"),
        TextKey::SettingsBackground => Some("Estilo"),
        TextKey::SettingsOverlayGroup => Some("Sobreposições"),
        TextKey::SettingsWireframeColor => Some("Cor do aramado"),
        TextKey::SettingsWireframeOpacity => Some("Opacidade do aramado"),
        TextKey::SettingsXrayOpacity => Some("Opacidade do raio-X"),
        TextKey::SettingsSolidColor => Some("Cor sólida da cabeça"),
        TextKey::SettingsLanguageGroup => Some("Idioma"),
        TextKey::SettingsMorphNames => Some("Nomes dos morphs nativos"),
        TextKey::SettingsMorphNamesTranslated => Some("Traduzidos"),
        TextKey::SettingsMorphNamesOriginal => Some("Original em inglês"),
        TextKey::SettingsLanguage => Some("Idioma da interface"),
        TextKey::SettingsFoldersGroup => Some("Pastas"),
        TextKey::SettingsVaMRoot => Some("Caminho de instalação do VaM"),
        TextKey::HairNeedsFinishedHead => Some("O cabelo é usado numa cabeça finalizada."),
        TextKey::NoMatchingHair => Some("Nenhum preset de cabelo correspondente"),
        TextKey::MorphSearch => Some("Buscar morphs"),
        TextKey::MorphOneSidedFilter => Some("E/D"),
        TextKey::MorphOneSidedFilterShow => Some("Mostrar morphs que movem apenas um lado"),
        TextKey::MorphOneSidedFilterHide => Some("Ocultar morphs que movem apenas um lado"),
        TextKey::AppearanceSearch => Some("Buscar looks"),
        TextKey::AppearanceLayers => Some("Camadas de aparencia"),
        TextKey::AddAppearanceLayer => Some("Adicionar a aparencia selecionada"),
        TextKey::AppearanceLayersEmpty => Some("Sem camadas ainda"),
        TextKey::AppearanceLayerRaise => Some("Subir"),
        TextKey::AppearanceLayerLower => Some("Descer"),
        TextKey::LookFind => Some("Carregar look"),
        TextKey::CameraTrackballArmed => {
            Some("Trackball · arraste para girar livremente · clique ou R para sair")
        }
        TextKey::HelpTrackball => Some("Rotação trackball"),
        TextKey::ShortcutTrackball => Some("R"),
        TextKey::SplitModelViewTooltip => Some("Ver de dois ângulos — dividir a vista"),
        TextKey::SplitModelViewName => Some("Vista dividida"),
        TextKey::HelpLevelRoll => Some("Nivelar o horizonte"),
        TextKey::ShortcutLevelRoll => Some("Alt+R"),
        TextKey::RecoveryTitle => Some("A última sessão terminou inesperadamente"),
        TextKey::RecoveryBody => Some(
            "Seus morphs, a escultura e as camadas de textura foram salvos logo antes da parada. Recuperar?",
        ),
        TextKey::RecoveryRestore => Some("Restaurar"),
        TextKey::RecoveryDiscard => Some("Começar do zero"),
        TextKey::RecoveryRestored => Some("Sessão anterior restaurada."),
        TextKey::RecoveryLookMissing => Some(
            "O look daquela sessão ainda não está na lista — as texturas foram restauradas e as edições se aplicarão ao selecioná-lo.",
        ),
        TextKey::SettingsVignetteGroup => Some("Vinheta"),
        TextKey::SettingsBrushSweepGroup => Some("Gesto de tamanho do pincel"),
        TextKey::BrushSweepBlender => Some("Blender"),
        TextKey::BrushSweepZBrush => Some("ZBrush"),
        TextKey::BrushSweepTooltip => {
            Some("Blender confirma com um clique ou segunda tecla; ZBrush ao soltar a tecla")
        }
        TextKey::HairPackageStyle => Some("Empacotar como .var"),
        TextKey::HairPackageDone => Some("Cabelo empacotado"),
        TextKey::HairPackageFailed => Some("Falha ao empacotar"),
        TextKey::HairPackageNeedsInstall => Some("Instale o estilo primeiro"),
        TextKey::DialogOpenHeadPreset => Some("Abrir uma predefinição de cabeça"),
        TextKey::FindHeadPreset => Some("Procurar predefinição…"),
        TextKey::SettingsVignetteIntensity => Some("Quantidade"),
        TextKey::SettingsVignetteSmoothness => Some("Suavidade"),
        TextKey::SettingsVignetteRoundness => Some("Arredondamento"),
        TextKey::SculptNotCarried => Some("A escultura não pôde acompanhar este look"),
        TextKey::SettingsToneCurve => Some("Curva tonal"),
        TextKey::SettingsToneCurveTooltip => Some("Ajusta as sombras e as altas luzes"),
        TextKey::SettingsVignetteTooltip => Some("Escurece as bordas do quadro"),
        TextKey::SettingsEffectEnabled => Some("Ativado"),
        TextKey::SettingsInterfaceGroup => Some("Interface"),
        TextKey::SettingsTooltips => Some("Mostrar dicas"),
        TextKey::SettingsResetGroup => Some("Redefinir"),
        TextKey::SettingsClearCache => Some("Esvaziar o cache"),
        TextKey::SettingsClearCacheTooltip => Some(
            "Bases extraídas e bancos de morphs, refeitos quando necessário. Esvaziar custa apenas um arranque mais lento.",
        ),
        TextKey::SettingsResetAll => Some("Redefinir todas as configurações e reiniciar"),
        TextKey::SettingsGraphicsLighting => Some("Iluminação"),
        TextKey::SettingsGraphicsEffects => Some("Efeitos"),
        TextKey::SettingsGraphicsQuality => Some("Qualidade"),
        TextKey::SettingsQualityAntialiasing => Some("Suavização"),
        TextKey::SettingsMsaa => Some("MSAA"),
        TextKey::SettingsMsaaTooltip => Some(
            "Suaviza bordas serrilhadas conforme as amostras escolhidas. Mais alto é mais limpo e mais custoso.",
        ),
        TextKey::SettingsMsaaRestart => Some("Aplica ao reiniciar — atualmente"),
        TextKey::SoloViewHint => Some("Mostrar apenas esta parte"),
        TextKey::SoloViewRestoreHint => Some("Restaurar a vista anterior"),
        TextKey::ToneCurveFilmic => Some("Fílmica"),
        TextKey::ToneCurveSoft => Some("Suave"),
        TextKey::LooksHiddenForOtherFigure => Some("Os looks da outra figura estão ocultos"),
        TextKey::LookBelongsToOtherFigure => Some("Este look foi feito para a outra figura"),
        TextKey::MorphEdit => Some("Editar morphs"),
        TextKey::VaMMorphPairImported => Some("Par de morphs do VaM importado"),
        TextKey::VaMMorphPairRejected => Some("Não é possível importar o par de morphs do VaM"),
        TextKey::MorphCategoryAll => Some("Todos"),
        TextKey::MorphCategoryEyes => Some("Olhos"),
        TextKey::MorphCategoryBrows => Some("Sobrancelhas"),
        TextKey::MorphCategoryNose => Some("Nariz"),
        TextKey::MorphCategoryMouth => Some("Boca"),
        TextKey::MorphCategoryJaw => Some("Mandíbula"),
        TextKey::MorphCategoryCheeks => Some("Bochechas"),
        TextKey::MorphCategoryEars => Some("Orelhas"),
        TextKey::MorphCategoryHead => Some("Rosto"),
        TextKey::MorphCategoryExpression => Some("Expressão"),
        TextKey::NoMatchingMorphs => Some("Nenhum morph correspondente"),
        TextKey::ResetMorphs => Some("Redefinir tudo"),
        TextKey::UndoMorphReset => Some("Desfazer redefinição"),
        TextKey::LightRotationGesture => Some("Shift+Arrastar botão direito"),
        TextKey::UpdateAvailable => Some("Atualizar"),
        TextKey::PackageFromFiles => Some("Escolher arquivos"),
        TextKey::PackageMorphFiles => Some("Morphs"),
        TextKey::PackageTextureFiles => Some("Texturas"),
        TextKey::PackageListEmpty => Some("Nada adicionado ainda"),
        TextKey::LightingStudio => Some("Estúdio"),
        TextKey::LightingSoft => Some("Suave"),
        TextKey::LightingDaylight => Some("Luz do dia"),
        TextKey::LightingWarm => Some("Quente"),
        TextKey::LightingGloss => Some("Verificar brilho"),
        TextKey::LightingPortrait => Some("Retrato"),
        TextKey::ImportLoadingMesh => Some("Carregando malha"),
        TextKey::ImportSimplifying => Some("Simplificando malha"),
        TextKey::StageOptimizeHead => Some("Otimizar cabeça"),
        TextKey::StagePrepareInput => Some("Preparar entrada"),
        TextKey::StageAlignScan => Some("Alinhar digitalização"),
        TextKey::StageFitShape => Some("Ajustar forma G2"),
        TextKey::StageValidate => Some("Validar estrutura"),
        TextKey::StagePrepareResult => Some("Preparar resultado"),
        TextKey::HeavyMeshNote => Some("cerca de {count} triangulos -- quanto mais, mais demora"),
        TextKey::TransformGroups => Some("Partes"),
        TextKey::CollapseTransformGroups => Some("Recolher partes"),
        TextKey::ExpandTransformGroups => Some("Expandir partes"),
        TextKey::Skin => Some("Pele"),
        TextKey::SculptTear => Some("Filme lacrimal"),
        TextKey::SculptLips => Some("Lábios"),
        TextKey::SculptTeethTongue => Some("Dentes · Língua"),
        TextKey::SculptInnerMouth => Some("Interior da boca"),
        TextKey::SculptEyes => Some("Olhos"),
        TextKey::Size => Some("Tamanho"),
        TextKey::SculptXSymmetry => Some("Simetria X"),
        TextKey::SculptXSymmetryTooltip => Some("Aplica o mesmo traço nos dois lados"),
        TextKey::BackfaceProtection => Some("Proteger verso"),
        TextKey::AutoGroup => Some("Grupo automático"),
        TextKey::AutoGroupTooltip => Some("Deforma apenas dentro de um grupo de polígonos"),
        TextKey::EyeTrackingAuto => Some("Auto"),
        TextKey::EyeGazeFreeze => Some("Fixar como morph"),
        TextKey::EyeGazeFreezeTooltip => Some(
            "Trava o olhar atual nos morphs, para que a aba Salvar exporte esta pose dos olhos",
        ),
        TextKey::Reset => Some("Redefinir"),
        TextKey::SculptGrab => Some("Agarrar"),
        TextKey::SculptSmooth => Some("Suavizar"),
        TextKey::SculptPushPull => Some("Empurrar · Puxar"),
        TextKey::ShortcutSculptGrab => Some("Arrastar"),
        TextKey::ShortcutSculptSmooth => Some("Shift + arrastar"),
        TextKey::ShortcutSculptPushPull => Some("Ctrl / Alt + arrastar"),
        TextKey::ResetSculpt => Some("Redefinir deformação"),
        #[cfg(test)]
        TextKey::SculptApplied => Some("Edições de escultura aplicadas"),
        TextKey::SculptFailed => Some("Falha na operação de escultura"),
        TextKey::Ready => Some("Pronto"),
        TextKey::Busy => Some("A edição fica bloqueada durante a geração"),
        TextKey::NeedScan => Some(
            "Abra primeiro uma cabeça escaneada OBJ, GLB ou FBX, ou comece por um look já instalado no VaM",
        ),
        TextKey::ScanLoadFailed => Some("Não foi possível ler esse arquivo de cabeça"),
        TextKey::NeedEyeMorph => Some("O morph de olhos nativo não é compatível com este G2"),
        TextKey::ReviewEyePins => Some("Revise os pontos vermelhos das pálpebras"),
        TextKey::ResultUnavailable => Some("Ajuste o rosto primeiro"),
        TextKey::MorphNotInLibrary => {
            Some("Esse morph ainda não está na biblioteca; o catálogo está carregando.")
        }
        TextKey::SculptNeedsBaseHead => Some("Primeiro continue com a cabeça base na aba Criar."),
        TextKey::MorphUnavailable => Some("Nenhum morph de resultado compatível disponível"),
        TextKey::AlignmentPending => {
            Some("Carregue uma cabeça escaneada e selecione sua pasta do VaM")
        }
        TextKey::ScanLoaded => Some("Cabeça personalizada OBJ, GLB ou FBX carregada"),
        TextKey::ScanUnloaded => Some("Arquivo de cabeça removido; a base G2 está pronta"),
        TextKey::PinPairsMismatched => Some("A contagem de pinos não confere; pinos sem par"),
        TextKey::UnloadScanTooltip => Some("Remover este arquivo de cabeça"),
        TextKey::TemplateLoaded => Some("G2 carregado"),
        TextKey::PinsReset => Some("Todos os pontos foram redefinidos"),
        TextKey::ResultStale => Some("As alterações exigem regenerar o resultado"),
        TextKey::GenerationStarted => Some("Ajustando o rosto"),
        TextKey::Cancelling => Some("Cancelando o ajuste do rosto"),
        TextKey::GenerationComplete => Some("Ajuste do rosto concluído"),
        TextKey::GenerationCancelled => Some("Ajuste do rosto cancelado"),
        TextKey::GenerationFailed => Some("Falha no ajuste do rosto"),
        TextKey::ExportStarted => Some("Exportando"),
        TextKey::ExportComplete => Some("Exportação concluída"),
        TextKey::ExportFailed => Some("Falha na exportação"),
        TextKey::MorphNameHint => Some("Digite um nome de morph"),
        TextKey::MorphGroupHint => Some("Grupo  (ex.: Custom, Morphs/Morph Loader)"),
        TextKey::MorphRegionHint => Some("Região  (ex.: Custom, Morphs/Body)"),
        TextKey::DirectEntry => Some("Digitar"),
        TextKey::VaMMetadataPoseMorph => Some("Registrar como Pose Morph"),
        TextKey::VaMMetadataPoseMorphTooltip => {
            Some("É tratado junto com as expressões e as poses")
        }
        TextKey::VaMMetadataBoneCorrection => Some("Correção do esqueleto"),
        TextKey::VaMMetadataBoneCorrectionTooltip => {
            Some("Recentraliza os pivôs no esqueleto remodelado")
        }
        TextKey::NativeCorePending => {
            Some("A integração do núcleo de ajuste nativo está em andamento")
        }
        TextKey::MorphPreviewUpdated => Some("Prévia do morph atualizada"),
        TextKey::MorphsReset => Some("Morphs redefinidos"),
        TextKey::MorphsResetUndone => Some("Redefinição dos morphs desfeita"),
        TextKey::MorphEditUndone => Some("Ajuste do morph desfeito"),
        TextKey::MorphApplied => Some("Morph aplicado"),
        TextKey::HelpPlace => Some("Colocar ponto"),
        TextKey::HelpMove => Some("Mover ponto"),
        TextKey::HelpDelete => Some("Remover ponto"),
        TextKey::HelpXSymmetry => Some("Simetria X dos pontos"),
        TextKey::HelpSnapRotation => Some("Rotação com encaixe"),
        TextKey::HelpDragZoom => Some("Zoom por arrasto"),
        TextKey::HelpOrbit => Some("Orbitar"),
        TextKey::HelpPan => Some("Deslocar"),
        TextKey::HelpZoom => Some("Zoom"),
        TextKey::HelpLight => Some("Girar a luz"),
        TextKey::HelpFrameView => Some("Enquadrar cabeça"),
        TextKey::HelpUndo => Some("Desfazer"),
        TextKey::ShortcutPlace => Some("Clique esq."),
        TextKey::ShortcutMove => Some("Alt + arrastar"),
        TextKey::ShortcutDelete => Some("Clique dir."),
        TextKey::ShortcutXSymmetry => Some("X"),
        TextKey::ShortcutSnapRotation => Some("Shift / Ctrl + arrastar rotação"),
        TextKey::ShortcutDragZoom => Some("Ctrl + arrastar vertical com botão do meio"),
        TextKey::ShortcutOrbit => Some("Arrastar com botão do meio"),
        TextKey::ShortcutPan => Some("Shift + arrastar com botão do meio"),
        TextKey::ShortcutZoom => Some("Roda do mouse"),
        TextKey::ShortcutLight => Some("Shift + arrastar dir."),
        TextKey::ShortcutFrameView => Some("F"),
        TextKey::HelpSnapView => Some("Encaixar vista"),
        TextKey::ShortcutSnapView => Some("Alt + arrastar com a roda"),
        TextKey::HelpStandardViews => Some("Vistas padrão"),
        TextKey::HelpCameraProjection => Some("Perspectiva / Ortográfica"),
        TextKey::ShortcutStandardViews => Some("Numérico 1-9"),
        TextKey::ShortcutCameraProjection => Some("Numérico ."),
        TextKey::ShortcutUndo => Some("Ctrl+Z"),
        TextKey::HelpRedo => Some("Refazer"),
        TextKey::ShortcutRedo => Some("Ctrl+Y"),
        TextKey::HairPresetSection => Some("Predefinicoes de cabelo"),
        TextKey::HairPresetLoad => Some("Carregar"),
        TextKey::HairToolPick => Some("Escolher camada"),
        TextKey::HairToolPickHint => Some(
            "Clique numa mecha no viewport para ativar a camada dela. O pincel espera o proximo clique.",
        ),
        TextKey::HelpHairPick => Some("Escolher a camada sob o cursor"),
        TextKey::ShortcutHairPick => Some("V, ou Ctrl + clique com qualquer pincel"),
        TextKey::HelpHairSmooth => Some("Suavizar em vez de pentear"),
        TextKey::ShortcutHairSmooth => Some("Shift ao pentear"),
        TextKey::HairGroupScalp => Some("Couro cabeludo"),
        TextKey::HairScalpAlpha => Some("Recorte do couro cabeludo"),
        TextKey::HairScalpPanel => Some("Couro cabeludo"),
        TextKey::HairScalpMesh => Some("Malha do couro cabeludo"),
        TextKey::HairScalpCreate => Some("Criar couro cabeludo"),
        TextKey::HairScalpAbsent => Some("Este penteado ainda nao tem camada de couro cabeludo."),
        TextKey::HistoryBranchTitle => Some("As etapas seguintes serão perdidas"),
        TextKey::HistoryBranchProceed => Some("Editar mesmo assim"),
        TextKey::DoNotShowAgain => Some("Não mostrar novamente"),
        TextKey::SurfaceBaseUnblended => {
            Some("Alguns mapas PBR não puderam ser mesclados com a pele do VaM")
        }
        TextKey::DetailEditRedone => Some("A edição desfeita foi refeita"),
        TextKey::ShortcutBrushSmaller => Some("Pincel menor"),
        TextKey::ShortcutBrushLarger => Some("Pincel maior"),
        TextKey::ShortcutBrushSizeDrag => Some("Tamanho do pincel, arrastando"),
        TextKey::ShortcutBrushStrengthDrag => Some("Força do pincel, arrastando"),
        TextKey::ShortcutStencilCancel => Some("Cancelar a colocação do estêncil"),
        TextKey::ShortcutViewOrbit => Some("Orbitar a vista"),
        TextKey::ShortcutViewPan => Some("Deslocar a vista"),
        TextKey::ShortcutViewDolly => Some("Aproximar e afastar a vista"),
        TextKey::TemplatePending => {
            Some("Selecione sua pasta do VaM para preparar a base G2 automaticamente")
        }
        TextKey::ResultEmpty => Some("Nenhum morph preparado"),
        TextKey::ScaleLink => Some("Vincular escala"),
        TextKey::ScaleLinkTooltip => Some("Vincula os valores de escala X/Y/Z"),
        TextKey::CameraSettings => Some("Configurações da câmera"),
        TextKey::Perspective => Some("Perspectiva"),
        TextKey::Orthographic => Some("Ortográfica"),
        TextKey::Fov => Some("FOV"),
        TextKey::ResetCamera => Some("Redefinir câmera"),
        TextKey::WindowMinimize => Some("Minimizar"),
        TextKey::WindowMaximize => Some("Maximizar"),
        TextKey::WindowRestore => Some("Restaurar"),
        TextKey::WindowClose => Some("Fechar"),
        TextKey::TooltipShow => Some("Mostrar"),
        TextKey::TooltipHide => Some("Ocultar"),
        TextKey::TooltipLock => Some("Travar"),
        TextKey::TooltipUnlock => Some("Destravar"),
        TextKey::AdjustEyeGaze => Some("Ajustar o olhar"),
        TextKey::ScanLoading => Some("Carregando cabeça escaneada"),
        TextKey::TemplateLoading => Some("Carregando modelo G2"),
        TextKey::ResultLoading => Some("Carregando malha de resultado"),
        TextKey::VaMMorphPairLoading => Some("Carregando par de morphs do VaM"),
        TextKey::SystemLog => Some("Log do sistema"),
        TextKey::Strength => Some("Força"),
        TextKey::StrengthShort => Some("Força"),
        TextKey::SculptBrushMove => Some("Agarrar"),
        TextKey::SculptBrushSmooth => Some("Suavizar"),
        TextKey::SculptBrushRestore => Some("Restaurar"),
        TextKey::SculptBrushMoveTooltip => {
            Some("Puxa a superfície com o ponteiro; a força define o quanto ela acompanha")
        }
        TextKey::SculptBrushSmoothTooltip => Some("Suaviza o detalhe da superfície"),
        TextKey::SculptBrushRestoreTooltip => Some("Restaura em direção à forma base"),
        TextKey::SculptBrushMask => Some("Mascara"),
        TextKey::SculptBrushMaskTooltip => {
            Some("Recorta a camada de aparencia selecionada para mostrar a de baixo. Alt repinta")
        }
        TextKey::TextureLayers => Some("Camadas de textura"),
        TextKey::PackageSection => Some("Exportar VAR"),
        TextKey::PackageCreatorHint => Some("Criador"),
        TextKey::PackageNameHint => Some("Nome do pacote"),
        TextKey::PackageDescriptionHint => Some("Descrição"),
        TextKey::PackageCreditsHint => Some("Créditos"),
        TextKey::PackageInstructionsHint => Some("Instruções"),
        TextKey::PackageLinkHint => Some("Link de apoio"),
        TextKey::PackageSave => Some("Salvar VAR"),
        TextKey::PackageFromThisHead => Some("Gerar a partir desta cabeça"),
        TextKey::PackageAddMorphs => Some("Adicionar morphs"),
        TextKey::PackageAddTextures => Some("Adicionar texturas"),
        TextKey::PackageNeedsVamRestart => Some("É preciso reiniciar o VaM"),
        TextKey::PackageSavedTo => Some("Salvo em"),
        TextKey::FieldRequired => Some("(obrigatório)"),
        TextKey::FieldOptional => Some("(opcional)"),
        TextKey::PackageSaveTo => Some("Salvar em"),
        TextKey::PackageNothingToPack => {
            Some("Nada para empacotar. Ative esta cabeça ou adicione um arquivo.")
        }
        TextKey::PackageVersionMarker => Some("V"),
        TextKey::PackageCreatorRequired => Some("É necessário um nome de criador."),
        TextKey::PackageNameRequired => Some("É necessário um nome de pacote."),
        TextKey::PackageReplaceTitle => Some("Já existe um VAR com este nome"),
        TextKey::PackageReplaceBody => {
            Some("Substituir? As cenas que referenciam esta versão lerão o novo conteúdo.")
        }
        TextKey::LicenseByTooltip => Some("dê crédito ao autor"),
        TextKey::LicenseBySaTooltip => Some("dê crédito ao autor / compartilhe igual"),
        TextKey::LicenseByNdTooltip => Some("dê crédito ao autor / sem derivações"),
        TextKey::LicenseByNcTooltip => Some("dê crédito ao autor / sem uso comercial"),
        TextKey::LicenseByNcSaTooltip => {
            Some("dê crédito ao autor / sem uso comercial / compartilhe igual")
        }
        TextKey::LicenseByNcNdTooltip => {
            Some("dê crédito ao autor / sem uso comercial / sem derivações")
        }
        TextKey::LicensePcEaTooltip => {
            Some("acesso antecipado para apoiadores, sem redistribuição")
        }
        TextKey::AddTextureLayer => Some("Adicionar imagem"),
        TextKey::AddG2UvTextureLayer => Some("Adicionar mapa UV"),
        TextKey::AddTextureLayerTooltip => {
            Some("Coloca uma foto de rosto na cabeca e a edita como textura")
        }
        TextKey::AddG2UvTextureLayerTooltip => {
            Some("Adiciona uma textura face do G2 no seu proprio layout UV e a edita")
        }
        TextKey::AddTextureLayerHint => {
            Some("Adicione uma imagem de rosto ou textura G2 com o botão +.")
        }
        TextKey::BakingTextures => Some("Fazendo bake das texturas…"),
        TextKey::BakeTexturesForSave => Some("Fazer bake para salvar"),
        TextKey::TextureMetalRough => Some("Metálico / Rugosidade"),
        TextKey::TextureGlossSmooth => Some("Brilho / Suavidade"),
        TextKey::LayerOpacity => Some("Opacidade da camada"),
        TextKey::PinOpacity => Some("Opacidade dos pontos"),
        TextKey::TransferPins => Some("Transferir pontos"),
        TextKey::TransferPinsTooltip => {
            Some("Copia os pontos que você colocou para as imagens das outras camadas.")
        }
        TextKey::DeleteLayer => Some("Excluir camada"),
        TextKey::TextureNormalStrength => Some("Força do normal"),
        TextKey::TextureInvertScalar => Some("Inverter valores escalares"),
        TextKey::BlendNormal => Some("Normal"),
        TextKey::BlendMultiply => Some("Multiplicação"),
        TextKey::BlendScreen => Some("Divisão"),
        TextKey::BlendOverlay => Some("Sobreposição"),
        TextKey::MatchToneToLayerBelow => Some("Igualar o tom à camada de baixo"),
        TextKey::ImageMirror => Some("Espelhar esq./dir."),
        TextKey::ImageMirrorOff => Some("Nenhum"),
        TextKey::ImageMirrorToLeft => Some("Para a esquerda"),
        TextKey::ImageMirrorToRight => Some("Para a direita"),
        TextKey::ImageAdjustments => Some("Ajustes de imagem"),
        TextKey::Exposure => Some("Exposição"),
        TextKey::Contrast => Some("Contraste"),
        TextKey::Saturation => Some("Saturação"),
        TextKey::Hue => Some("Matiz"),
        TextKey::Temperature => Some("Temperatura"),
        TextKey::TextureMaskPreviewTooltip => {
            Some("Mostra a máscara da camada ativa como uma sobreposição vermelha translúcida")
        }
        TextKey::ClearLayerMask => Some("Limpar a máscara da camada"),
        TextKey::ResetSourceRetouch => Some("Redefinir o retoque da origem"),
        TextKey::CloneSampleHint => {
            Some("Alt+clique na imagem de origem para escolher a amostra de clonagem.")
        }
        TextKey::BakeBase => Some("Opções de bake"),
        TextKey::TransparentOverlay => Some("Bake apenas das camadas"),
        TextKey::CurrentSkinBake => Some("Bake com a pele do VaM"),
        TextKey::TransparentOverlayTooltip => {
            Some("Faz o bake apenas das imagens das camadas, sem o preset de pele")
        }
        TextKey::CurrentSkinBakeTooltip => {
            Some("Faz o bake das camadas sobre o preset de pele como base")
        }
        TextKey::SkinPresetRequired => {
            Some("Escolha primeiro um preset de pele nas configurações de pele")
        }
        TextKey::SkinSettings => Some("Configurações de pele"),
        TextKey::ScanHeadColor => Some("Cor da cabeça escaneada"),
        TextKey::G2HeadColor => Some("Cor da cabeça G2"),
        TextKey::MorphFilterAll => Some("Todos"),
        TextKey::MorphFilterActive => Some("Ativos"),
        TextKey::BrushDodge => Some("Clarear"),
        TextKey::BrushBurn => Some("Escurecer"),
        TextKey::BrushSaturate => Some("Saturar"),
        TextKey::BrushDesaturate => Some("Dessaturar"),
        TextKey::ShowLayersOnly => Some("Apenas imagens das camadas"),
        TextKey::ShowLayersOnlyTooltip => {
            Some("Oculta a pele do VaM e mostra apenas as imagens das camadas")
        }
        TextKey::MatchToneTooltip => {
            Some("Ajusta exposição, saturação e temperatura à camada abaixo")
        }
        TextKey::ClearLayerMaskTooltip => Some("Apaga a máscara desta camada"),
        TextKey::TextureMetalRoughTooltip => Some("Salva os mapas na convenção metallic/roughness"),
        TextKey::TextureGlossSmoothTooltip => {
            Some("Salva os mapas na convenção glossiness/smoothness do VaM")
        }
        TextKey::TextureInvertScalarTooltip => Some(
            "Inverte os valores quando a fonte usa a convenção oposta (roughness ↔ glossiness)",
        ),
        TextKey::BaseWithoutSkin => Some("Camada"),
        TextKey::BaseWithoutSkinTooltip => Some(
            "As camadas pintadas sobre a cor sólida, sem o preset; a textura exportada não muda",
        ),
        TextKey::SolidColorTooltip => Some("Apenas a cor sólida, sem textura"),
        TextKey::TextureSkinTooltip => Some("As camadas pintadas sobre o preset de pele do VaM"),
        TextKey::BoundaryFeather => Some("Suavizar a borda do rosto"),
        TextKey::BoundaryFeatherTooltip => Some(
            "Apenas difuso. Mapas como normal e rugosidade não podem ser mesclados por alfa, então a borda deles continua dura",
        ),
        TextKey::Baking => Some("Fazendo bake…"),
        TextKey::TextureToolPinPair => {
            Some("Par de pontos — coloque pontos correspondentes no rosto 3D e na imagem de origem")
        }
        TextKey::TextureToolMask => Some("Máscara — pinte para revelar; segure Alt para ocultar"),
        TextKey::TextureToolClone => {
            Some("Carimbo — Alt+clique em uma amostra e pinte para copiá-la")
        }
        TextKey::TextureToolDodgeBurn => {
            Some("Clarear/escurecer — clareia; segure Alt para escurecer")
        }
        TextKey::TextureToolSponge => Some("Esponja — adiciona saturação; segure Alt para remover"),
        TextKey::SelectTextureLayer => Some("Selecione uma camada de textura."),
        TextKey::LoadingTextureImage => Some("Carregando imagem…"),
        TextKey::ScanTextureAlignment => Some("A textura do scan herda o alinhamento 3D ajustado."),
        TextKey::NoTextureImage => Some("Sem imagem."),
        TextKey::MorphSavedReloadFemale => {
            Some("Pressione Reload Custom Morphs na aba Female Morphs do VaM para atualizar.")
        }
        TextKey::MorphSavedReloadMale => {
            Some("Pressione Reload Custom Morphs na aba Male Morphs do VaM para atualizar.")
        }
        TextKey::BootStarting => Some("Preparando o Vkit"),
        TextKey::BootFonts => Some("Carregando fontes"),
        TextKey::BootGraphics => Some("Iniciando os gráficos"),
        TextKey::BootSettings => Some("Carregando as configurações"),
        TextKey::BootTemplate => Some("Carregando o G2"),
        TextKey::BootWorkspace => Some("Preparando a área de trabalho"),
        TextKey::BootReady => Some("Abrindo"),
        TextKey::StartupFailedTitle => Some("O Vkit não conseguiu iniciar"),
        TextKey::StartupFailedGraphics => Some(
            "O Vkit desenha com DirectX 12, e os gráficos desta máquina não conseguiram fornecê-lo. Atualize o driver da placa de vídeo ou execute o Vkit em uma máquina com uma GPU compatível com DirectX 12. Uma sessão de área de trabalho remota ou uma máquina virtual costuma não ter uma tela DirectX 12 própria.",
        ),
        TextKey::StartupFailedOther => {
            Some("O Vkit parou durante a inicialização e não conseguiu abrir a janela.")
        }
        TextKey::StartupFailedLogHint => Some("O que deu errado foi escrito em:"),
        TextKey::DialogOpenScan => Some("Abrir cabeça OBJ, GLB ou FBX"),
        TextKey::DialogAddUvLayer => Some("Adicionar camada de textura G2 UV"),
        TextKey::DialogAddTextureLayer => Some("Adicionar camada de textura"),
        TextKey::DialogSaveMorphPair => Some("Salvar morph VaM VMI + VMB"),
        TextKey::DialogChooseVamFolder => Some("Escolher a pasta do Virt-A-Mate"),
        _ => None,
    }
}

pub(super) fn hair_label(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some("Quantidade de cabelo"),
        "curveDensity" => Some("Densidade da curva"),
        "iterations" => Some("Iterações"),
        "simulationEnabled" => Some("Física"),
        "collisionEnabled" => Some("Colisão"),
        "weight" => Some("Peso"),
        "drag" => Some("Resistência do ar"),
        "gravityMultiplier" => Some("Multiplicador de gravidade"),
        "collisionRadius" => Some("Raio de colisão"),
        "collisionRadiusRoot" => Some("Raio de colisão (raiz)"),
        "friction" => Some("Atrito"),
        "bendResistance" => Some("Resistência a dobrar"),
        "rootRigidity" => Some("Rigidez na raiz"),
        "mainRigidity" => Some("Rigidez no meio"),
        "tipRigidity" => Some("Rigidez na ponta"),
        "jointRigidity" => Some("Rigidez das juntas"),
        "rigidityRolloffPower" => Some("Queda de rigidez"),
        "cling" => Some("Coesão de estilo"),
        "clingRolloff" => Some("Queda de coesão"),
        "snap" => Some("Encaixe"),
        "usePaintedRigidity" => Some("Usar rigidez pintada"),
        "width" => Some("Espessura"),
        "length1" => Some("Comprimento 1"),
        "length2" => Some("Comprimento 2"),
        "length3" => Some("Comprimento 3"),
        "maxSpread" => Some("Dispersão máxima"),
        "spreadRoot" => Some("Dispersão (raiz)"),
        "spreadMid" => Some("Dispersão (meio)"),
        "spreadTip" => Some("Dispersão (ponta)"),
        "spreadMidpoint" => Some("Ponto médio de dispersão"),
        "spreadCurvePower" => Some("Curva de dispersão"),
        "curlScale" => Some("Tamanho do cacho"),
        "curlFrequency" => Some("Frequência do cacho"),
        "curlScaleRandomness" => Some("Aleatoriedade de tamanho"),
        "curlFrequencyRandomness" => Some("Aleatoriedade de frequência"),
        "curlRoot" => Some("Cacho (raiz)"),
        "curlMid" => Some("Cacho (meio)"),
        "curlTip" => Some("Cacho (ponta)"),
        "curlMidpoint" => Some("Ponto médio do cacho"),
        "curlCurvePower" => Some("Curva do cacho"),
        "curlX" => Some("Cacho X"),
        "curlY" => Some("Cacho Y"),
        "curlZ" => Some("Cacho Z"),
        "curlNormalAdjust" => Some("Ajuste de normal do cacho"),
        "curlAllowReverse" => Some("Permitir sentido inverso"),
        "curlAllowFlipAxis" => Some("Permitir eixo invertido"),
        "Alpha Adjust" => Some("Opacidade do couro"),
        "Diffuse Color" => Some("Cor difusa do couro"),
        "Gloss" => Some("Brilho do couro"),
        "Specular Intensity" => Some("Especular do couro"),
        "Global Illumination Filter" => Some("Iluminação global do couro cabeludo"),
        "Diffuse Texture Offset" => Some("Deslocamento de textura do couro"),
        "rootColor" => Some("Cor da raiz"),
        "tipColor" => Some("Cor da ponta"),
        "specularColor" => Some("Cor especular"),
        "colorRolloff" => Some("Queda de cor raiz→ponta"),
        "randomColorPower" => Some("Força da cor aleatória"),
        "randomColorOffset" => Some("Deslocamento da cor aleatória"),
        "primarySpecularSharpness" => Some("Brilho primário"),
        "secondarySpecularSharpness" => Some("Brilho secundário"),
        "specularShift" => Some("Deslocamento do brilho secundário"),
        "diffuseSoftness" => Some("Suavidade difusa"),
        "fresnelPower" => Some("Força de Fresnel"),
        "fresnelAttenuation" => Some("Atenuação de Fresnel"),
        "IBLFactor" => Some("Luz indireta"),
        "normalRandomize" => Some("Aleatoriedade das normais"),
        _ => None,
    }
}

pub(super) fn hair_hint(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some(
            "Quantas mechas crescem entre cada trio de guias. É a aparência do cabelo, e a maior parte do seu custo.",
        ),
        "curveDensity" => Some(
            "Pontos desenhados ao longo de cada mecha. Mais é mais suave e mais lento; a forma se lê muito antes de o número ficar alto.",
        ),
        "iterations" => Some(
            "Passagens do solver por quadro. Mais passagens seguram as restrições com mais firmeza em movimentos rápidos.",
        ),
        "simulationEnabled" => {
            Some("Se esta parte se move no jogo. Nada a ver com o interruptor da viewport.")
        }
        "collisionEnabled" => Some("Se as mechas desta parte são empurradas para fora do corpo."),
        "weight" => {
            Some("Com que peso uma mecha cai. Escala a gravidade e a resistência a ser movida.")
        }
        "drag" => Some("Quanto o ar segura uma mecha. Alto assenta depressa e balança pouco."),
        "gravityMultiplier" => Some("A gravidade sobre esta parte, como múltiplo da da cena."),
        "collisionRadius" => {
            Some("A espessura com que uma mecha colide, ao longo de todo o comprimento.")
        }
        "collisionRadiusRoot" => Some("A espessura perto da raiz, quando deva diferir do resto."),
        "friction" => Some("Quanto uma mecha agarra o que toca em vez de deslizar."),
        "bendResistance" => Some("Quanto uma mecha resiste a ser dobrada a partir da linha reta."),
        "rootRigidity" => Some("Com que força a raiz mantém a forma penteada."),
        "mainRigidity" => Some("Com que força o meio mantém a forma penteada."),
        "tipRigidity" => Some("Com que força a ponta mantém a forma penteada."),
        "jointRigidity" => Some(
            "A dureza das juntas de estilo: as molas que mantêm um rabo de cavalo como rabo de cavalo.",
        ),
        "rigidityRolloffPower" => Some(
            "Como as três rigidezes se misturam ao longo da mecha. Mais alto leva o valor da raiz mais para baixo.",
        ),
        "cling" => Some("Com que força as juntas de estilo puxam as mechas umas para as outras."),
        "clingRolloff" => Some(
            "Até onde a coesão se desvanece ao longo da mecha. Mais alto confina-a perto da raiz.",
        ),
        "snap" => Some("Com que firmeza cada segmento mantém o comprimento com que foi criado."),
        "usePaintedRigidity" => Some("Tirar a rigidez do mapa pintado em vez dos três controles."),
        "width" => Some(
            "A espessura com que uma mecha é desenhada. O controle do VaM também termina em um décimo de milímetro.",
        ),
        "length1" => {
            Some("Comprimento das mechas mais próximas da primeira guia do seu triângulo.")
        }
        "length2" => Some("Comprimento das mechas mais próximas da segunda guia do seu triângulo."),
        "length3" => {
            Some("Comprimento das mechas mais próximas da terceira guia do seu triângulo.")
        }
        "maxSpread" => Some(
            "Um teto para o quanto uma mecha pode afastar-se da sua guia, não um ganho. Depois de ultrapassar o espaçamento entre guias, aumentá-lo não faz nada.",
        ),
        "spreadRoot" => Some(
            "Com que liberdade as mechas se separam na raiz. Baixo agrupa-as em direção à guia.",
        ),
        "spreadMid" => Some("Com que liberdade as mechas se separam no meio."),
        "spreadTip" => Some("Com que liberdade as mechas se separam na ponta."),
        "spreadMidpoint" => Some("A partir de que ponto da mecha se aplica a dispersão do meio."),
        "spreadCurvePower" => Some("Com que brusquidão a dispersão vira entre raiz, meio e ponta."),
        "curlScale" => {
            Some("A largura do cacho: o raio em torno do qual se enrola, não o comprimento.")
        }
        "curlFrequency" => Some("Voltas por centímetro de mecha."),
        "curlScaleRandomness" => Some("Quanto a largura do cacho varia de mecha para mecha."),
        "curlFrequencyRandomness" => Some("Quanto o ritmo das voltas varia de mecha para mecha."),
        "curlRoot" => Some("Quanto cacho chega à raiz."),
        "curlMid" => Some("Quanto cacho chega ao meio."),
        "curlTip" => Some("Quanto cacho chega à ponta."),
        "curlMidpoint" => Some("A partir de que ponto da mecha se aplica o cacho do meio."),
        "curlCurvePower" => Some("Com que brusquidão o cacho vira entre raiz, meio e ponta."),
        "curlX" => Some(
            "O deslocamento do cacho ao longo de X. Um vetor que é rotacionado, não um eixo em torno do qual enrolar.",
        ),
        "curlY" => Some("O deslocamento do cacho ao longo de Y."),
        "curlZ" => Some("O deslocamento do cacho ao longo de Z."),
        "curlNormalAdjust" => Some(
            "Somado ao deslocamento do cacho ao longo da normal. Levanta a espiral da cabeça em vez de incliná-la.",
        ),
        "curlAllowReverse" => Some(
            "Deixa algumas mechas enrolarem ao contrário, para que uma cabeleira cacheada não se leia como uma mecha repetida.",
        ),
        "curlAllowFlipAxis" => {
            Some("Deixa algumas mechas enrolarem em torno de um eixo invertido, pela mesma razão.")
        }
        "Alpha Adjust" => Some(
            "Quão opaca é a touca do couro cabeludo. A maioria das partes veste a touca de outro e a mantém no mínimo para escondê-la; aumente só quando uma risca precisar mostrar o couro.",
        ),
        "Diffuse Color" => Some(
            "A cor difusa da própria touca. O VaM a entrega em branco, para que a pele por baixo apareça.",
        ),
        "Gloss" => Some("Quão brilhante é a touca onde ela aparece."),
        "Specular Intensity" => Some("Com que força a touca recebe um brilho."),
        "Global Illumination Filter" => Some("Quanta luz ambiente a touca capta."),
        "Diffuse Texture Offset" => {
            Some("Somado à cor difusa da touca. Apesar do nome, não é um índice de folha.")
        }
        "rootColor" => Some("A cor na raiz."),
        "tipColor" => Some("A cor na ponta."),
        "specularColor" => Some("A cor do brilho."),
        "colorRolloff" => Some("Como a cor da raiz cede à cor da ponta ao longo da mecha."),
        "randomColorPower" => Some("Com que força as mechas variam de cor entre si."),
        "randomColorOffset" => {
            Some("Para que lado essa variação pende, mais clara ou mais escura.")
        }
        "primarySpecularSharpness" => {
            Some("Quão fechado é o brilho principal. Mais alto é mais lustroso.")
        }
        "secondarySpecularSharpness" => {
            Some("Quão fechado é o segundo brilho: o colorido que viaja com o cabelo.")
        }
        "specularShift" => {
            Some("A que distância do primeiro o segundo brilho fica ao longo da mecha.")
        }
        "diffuseSoftness" => Some(
            "Com que suavidade a luz envolve uma mecha em vez de iluminar só o lado voltado para ela.",
        ),
        "fresnelPower" => {
            Some("Com que brusquidão a borda clareia onde o cabelo se afasta da vista.")
        }
        "fresnelAttenuation" => Some("A força desse clareamento da borda."),
        "IBLFactor" => Some("Quanta luz ambiente da cena o cabelo recebe."),
        "normalRandomize" => Some(
            "Quanto a normal de sombreamento de cada mecha é sacudida, para que vizinhas não sombreiem igual.",
        ),
        _ => None,
    }
}
