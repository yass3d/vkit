use super::TextKey;

pub(super) const fn label(key: TextKey) -> Option<&'static str> {
    #[allow(unreachable_patterns, reason = "the fallback outlives today's key set")]
    match key {
        TextKey::SurfaceSmooth => Some("メッシュスムーズ"),
        TextKey::SurfaceSmoothTooltip => Some(
            "VaM 互換のレンダリングスムージングです。0 は編集用のベースメッシュ、3 は VaM の既定値、4 が最大です。モーフ、UV、スカルプト、テクスチャベイクはベーストポロジーを保ちます",
        ),
        TextKey::FaceMatch => Some("生成"),
        TextKey::ResultPreview | TextKey::Save => Some("保存"),
        TextKey::TextureStage => Some("テクスチャ"),
        TextKey::HairStage => Some("ヘア"),
        TextKey::HairUnavailable => Some("ヘアを編集するには、まずスカルプト結果が必要です"),
        TextKey::HairPartsPanel => Some("ヘアパーツ"),
        TextKey::AddHairPart => Some("ヘアパーツを追加"),
        TextKey::HairScalpMeshCrowded => {
            Some("新しい頭皮メッシュに空きがなく、一部の毛束を移せませんでした")
        }
        TextKey::HairScalpMissing => {
            Some("スカルプメッシュがありません。VaMルートを開いて再スキャンしてください")
        }
        TextKey::HairShowPoints => Some("ポイント表示"),
        TextKey::HairShowPointsHint => Some(
            "頭皮の植え込みポイントを表示します。髪だけを見たいときはオフに。ブラシはどちらでも動作します。",
        ),
        TextKey::HairPartTint => Some("パートの色分け"),
        TextKey::HairPartTintHint => Some(
            "パートごとに異なるティントを付け、統合スタイルでどの毛束がどのパートか見分けられます。表示のみで、書き出しには影響しません。",
        ),
        TextKey::HairSettingsPanel => Some("ヘア設定"),
        TextKey::HairCreateFirst => Some("ヘアを作成してください。"),
        TextKey::HairHideStrands => Some("髪を隠す"),
        TextKey::HairHideStrandsHint => {
            Some("生えた髪を外してストリームだけを見ます。オンにするとストリーム表示も入ります。")
        }
        TextKey::HairShowStreams => Some("ヘアストリーム表示"),
        TextKey::HairShowStreamsHint => {
            Some("実際に編集する毛束を、そこから生えた髪の上に描きます。")
        }
        TextKey::HairViewportPhysics => Some("ビューポート物理"),
        TextKey::HairViewportPhysicsHint => {
            Some("画面上で物理をプレビューします。エクスポートとは関係ありません。")
        }
        TextKey::HairMirrorEdit => Some("ミラー編集"),
        TextKey::HairMirrorEditHint => Some("ブラシが左右対称に両側を同時に編集します。"),
        TextKey::HairAutoPart => Some("パーツ自動編集"),
        TextKey::HairAutoPartHint => {
            Some("アクティブレイヤーの代わりに、カーソル下で認識したパーツのみ編集します。")
        }
        TextKey::HairExportSection => Some("ヘアを保存"),
        TextKey::HairExportBothSexes => Some("共用"),
        TextKey::HairExportRescanNotice => {
            Some("VaM - Hair - Rescan Files を押すとヘアリストに反映されます")
        }
        TextKey::HairExportOverwroteNotice => {
            Some("上書きしたサムネイルは Hard Reset か VaM 再起動後に反映されます")
        }
        TextKey::HairSimToggleHint => Some("ビューポートとゲームの両方でヘア物理を実行します。"),
        TextKey::HairCollisionToggleHint => {
            Some("シミュレーション中に髪が頭や体を貫通しないようにします。")
        }
        TextKey::HairStyleJoints => Some("スタイルジョイント"),
        TextKey::HairStyleJointsHint => Some(
            "近い毛束同士をバネで結び、ゲーム内で形を保ちます。密着(Cling)が調整する対象です。長髪・結んだ髪向けで、短髪ではオフ推奨。",
        ),
        TextKey::HairThumbnailPrompt => Some("アングルを合わせてクリックで撮影"),
        TextKey::HairThumbnailShoot => Some("撮影"),
        TextKey::HairThumbnailSkip => Some("スキップ"),
        TextKey::HairThumbnailSaved => Some("サムネイルを保存しました"),
        TextKey::HairThumbnailFailed => Some("サムネイルの保存に失敗しました"),
        TextKey::HairGameOnly => Some(
            "物理設定:ビューポートはシミュレーションを行わないため、VaM内でのみ反映されます。スタイルにはそのまま書き出されます。",
        ),
        TextKey::HairPartVisible => Some("このヘアパーツの表示/非表示"),
        TextKey::RemoveHairPart => Some("ヘアパーツを削除"),
        TextKey::HairRenamePart => Some("クリックで名前を変更"),
        TextKey::HairToolPlant => Some("植毛"),
        TextKey::HairToolErase => Some("消去"),
        TextKey::HairToolGrow => Some("伸ばす (Altで縮める)"),
        TextKey::HairToolComb => Some("とかす"),
        TextKey::HairToolPlantHint => Some("ブラシ下の頭皮頂点に毛を植えます"),
        TextKey::HairToolGrowHint => Some("ブラシ下の毛束を伸ばします。Altで縮みます"),
        TextKey::HairToolEraseHint => Some("ブラシ下の毛束を消します"),
        TextKey::HairToolCombHint => Some("ブラシ下の毛束を梳きます。根元は固定されます"),
        TextKey::HairToolPinch => Some("まとめる"),
        TextKey::HairToolPinchHint => Some("ブラシ下の毛束をまとめます。Altで広げます"),
        TextKey::HairToolCut => Some("カット"),
        TextKey::HairToolCutHint => Some("ブラシで引いた位置で毛先を切りそろえます"),
        TextKey::HairToolPuff => Some("ふくらませる"),
        TextKey::HairToolPuffHint => Some("ブラシ下の毛を肌から立てます。Altで寝かせます"),
        TextKey::HairToolVertex => Some("ポイント"),
        TextKey::HairToolVertexHint => Some("毛束の関節を1つ掴んで動かします。長さは保たれます"),
        TextKey::HairCopySettings => Some("すべてコピー"),
        TextKey::HairPasteSettings => Some("すべて貼り付け"),
        TextKey::HairGroupPerformance => Some("パフォーマンス"),
        TextKey::HairGroupPhysics => Some("物理"),
        TextKey::HairGroupStiffness => Some("剛性"),
        TextKey::HairGroupShape => Some("形状"),
        TextKey::HairGroupCurl => Some("カール"),
        TextKey::HairGroupLook => Some("見た目"),
        TextKey::HairColorScalp => Some("頭皮"),
        TextKey::HairColorRoot => Some("根元"),
        TextKey::HairColorTip => Some("毛先"),
        TextKey::HairColorSpecular => Some("光沢"),
        TextKey::HairScalpMask => Some("頭皮テクスチャ"),
        TextKey::HairScalpMaskBuiltIn => Some("内蔵"),
        TextKey::HairScalpMaskCustom => Some("ファイルから..."),
        TextKey::HairExport => Some("VaMへ書き出し"),
        TextKey::HairExportName => Some("アイテム名"),
        TextKey::HairExportCreator => Some("作成者"),
        TextKey::HairExportNeedsMetadata => Some("書き出す前にアイテム名と作者を入力してください"),
        TextKey::HairOverwriteTitle => Some("このスタイルを置き換えますか"),
        TextKey::HairOverwriteBody => Some(
            "同じ名前と作者のスタイルが既にあります。書き出すとファイルを置き換えます。VaMは再起動まで読み込み済みのものを使い続けます。",
        ),
        TextKey::HairOverwriteProceed => Some("置き換える"),
        TextKey::HairExportDone => Some("ヘアアイテムを保存しました"),
        TextKey::HairExportFailed => Some("ヘアの書き出しに失敗"),
        TextKey::HairExportNeedsPart => Some("先にストランドのあるヘアパーツを選択してください"),
        TextKey::HairExportNeedsVaMRoot => Some("書き出す前にVaMルートを設定してください"),
        TextKey::HairMirrorPart => Some("パーツを反対側へ反転"),
        TextKey::HairDuplicatePart => Some("パーツを複製"),
        TextKey::HairSegmentsShort => Some("セグ"),
        TextKey::DetailCorrection => Some("編集"),
        TextKey::MorphCategoryBody => Some("体"),
        TextKey::TextureNamePlaceholder => Some("テクスチャ名を入力"),
        TextKey::TextureOverwriteTitle => Some("同名のテクスチャを上書きします"),
        TextKey::OpenScan => Some("スキャンヘッド"),
        TextKey::SourceMorphMissing => Some("このルックのモーフが一つもインストールされていません"),
        TextKey::EditDetails => Some("詳細を編集"),
        TextKey::Female => Some("女性"),
        TextKey::Male => Some("男性"),
        TextKey::Lighting => Some("ライティング"),
        TextKey::Brightness => Some("明るさ"),
        TextKey::LightRotation => Some("ライト回転"),
        TextKey::AddHeadFileHint => Some("OBJ・GLB・FBX ファイルをここにドラッグ"),
        TextKey::AddHeadFile => Some("ヘッドファイルを追加してください"),
        TextKey::AutoAlign => Some("自動フィット"),
        TextKey::PlacementReset => Some("配置リセット"),
        TextKey::Scale => Some("スケール"),
        TextKey::Position => Some("位置"),
        TextKey::Rotation => Some("回転"),
        TextKey::XMirror => Some("X ミラー"),
        TextKey::XMirrorTooltip => Some("ピンを置くと反対側の同じ位置にも置かれます"),
        TextKey::NumbersTooltip => Some("ピンに順番を表示します"),
        TextKey::XMirrorRejected => Some("対称点を作成できないため X ミラーを無効にしました"),
        TextKey::ScanFidelity => Some("スキャン再現度"),
        TextKey::ScanOptions => Some("スキャン設定"),
        TextKey::RestoreNeckEars => Some("首と耳を復元"),
        TextKey::RestoreNeckEarsTooltip => {
            Some("耳・うなじ・後頭部をG2の原形に保ち、顔側にだけ変形をなじませます")
        }
        TextKey::RestoreNeckEarsDeferred => {
            Some("スカルプトの手が入っているため、次の生成時に適用されます")
        }
        TextKey::ScanFidelityTooltip => Some(
            "1 より上げるとスキャンの形をより追い、下げると G2 本来の滑らかさを保ちます。2 までは形だけですが、それを超えるとスキャンのノイズ(斑点・穴・継ぎ目)まで再現され始めます。ピンはどの値でも無視されず、判定基準も変わりません。",
        ),
        TextKey::ScanSymmetry => Some("スキャンヘッド対称"),
        TextKey::Original => Some("オリジナル"),
        TextKey::Left => Some("左"),
        TextKey::Right => Some("右"),
        TextKey::EyesOpen => Some("開いた目"),
        TextKey::EyesClosed => Some("閉じた目"),
        TextKey::View => Some("表示オプション"),
        TextKey::On => Some("オン"),
        TextKey::Numbers => Some("番号"),
        TextKey::PinPairsPlaced => Some("ピン"),
        TextKey::PinPlacement => Some("ピンの配置"),
        TextKey::ScanReference => Some("スキャン基準"),
        TextKey::AddHeadFileAction => Some("ヘッドファイルを追加"),
        TextKey::AddHeadFileActionTooltip => {
            Some("スキャンしたヘッドファイル(OBJ · GLB · FBX)を読み込みます")
        }
        TextKey::ContinueStep => Some("ベースヘッドで進む"),
        TextKey::ContinueStepTooltip => Some(
            "スキャンなしで G2 ヘッドをそのまま編集します。後からスキャンを読み込んで合わせることもできます",
        ),
        TextKey::Undo => Some("元に戻す"),
        TextKey::ResetAllPins => Some("全ピンをリセット"),
        TextKey::ResetAll => Some("すべて初期化"),
        TextKey::MorphSave => Some("モーフを保存"),
        TextKey::TextureSaveSection => Some("テクスチャを保存"),
        TextKey::Generate => Some("顔フィッティング"),
        TextKey::Cancel => Some("キャンセル"),
        TextKey::CopyAll => Some("すべてコピー"),
        TextKey::Next => Some("保存する"),
        TextKey::TextureSkin => Some("プリセット"),
        TextKey::SolidColor => Some("ソリッド"),
        TextKey::BackfaceProtectionTooltip => Some("薄いメッシュの裏面を保護します"),
        TextKey::WireframeColor => Some("ワイヤー色"),
        TextKey::WireframeSettings => Some("ワイヤーフレーム"),
        TextKey::XraySettings => Some("X-ray"),
        TextKey::ViewportLightingTooltip => Some("ライトを調整"),
        TextKey::Background => Some("背景"),
        TextKey::ViewportBackgroundTooltip => Some("背景と参照画像を調整"),
        TextKey::BackgroundRadial => Some("放射状"),
        TextKey::BackgroundVertical => Some("縦方向"),
        TextKey::BackgroundFlat => Some("単色"),
        TextKey::ViewportWireframeTooltip => Some("ワイヤーフレームの表示と調整"),
        TextKey::ViewportXrayTooltip => Some("X-ray オーバーレイの表示と調整"),
        TextKey::ViewportSkinTooltip => Some("VaM スキンテクスチャを選択"),
        TextKey::FalloffTooltip => Some("ブラシの減衰カーブを選択します"),
        TextKey::FalloffSmooth => Some("スムーズ"),
        TextKey::FalloffSmoother => Some("よりスムーズ"),
        TextKey::FalloffSharp => Some("シャープ"),
        TextKey::FalloffLinear => Some("リニア"),
        TextKey::PlaceHead => Some("ヘッドの位置合わせ"),
        TextKey::PlaceHeadTooltip => {
            Some("読み込んだヘッドをG2に重ねて位置・回転・サイズを合わせます。")
        }
        TextKey::ScanOverlay => Some("重ね表示"),
        TextKey::OverlayDone => Some("重ね合わせ完了"),
        TextKey::ProjectImage => Some("投影"),
        TextKey::ProjectionCanvasHint => Some("3D ビューで塗るとこの UV キャンバスに現れます"),
        TextKey::HelpProjectionPaint => Some("塗る（投影）"),
        TextKey::ShortcutProjectionPaint => Some("左ドラッグ"),
        TextKey::HelpProjectionMove => Some("画像を移動"),
        TextKey::ShortcutProjectionMove => Some("右ドラッグ"),
        TextKey::HelpProjectionRotate => Some("画像を回転"),
        TextKey::ShortcutProjectionRotate => Some("Shift+右ドラッグ"),
        TextKey::HelpProjectionZoom => Some("画像の拡大縮小"),
        TextKey::ShortcutProjectionZoom => Some("ホイール"),
        TextKey::HelpBrushSize => Some("ブラシサイズ"),
        TextKey::ShortcutBrushSize => Some("F ドラッグ · [ ]"),
        TextKey::HelpBrushStrength => Some("ブラシ強度"),
        TextKey::ShortcutBrushStrength => Some("Shift+F ドラッグ"),
        TextKey::HelpProjectionDone => Some("投影を終了"),
        TextKey::ShortcutProjectionDone => Some("確認 · Esc"),
        TextKey::ProjectDone => Some("確認"),
        TextKey::ProjectDoneTooltip => {
            Some("投影を終了します。塗った結果はレイヤーにそのまま残ります")
        }
        TextKey::MorphCompare => Some("比較"),
        TextKey::Opacity => Some("不透明度"),
        TextKey::G2Head => Some("G2 ヘッド"),
        TextKey::Eyelashes => Some("まつ毛"),
        TextKey::OverwriteTitle => Some("ファイルの上書き"),
        TextKey::OverwriteBody => Some("同名のファイルが既に存在します。上書きしますか？"),
        TextKey::Overwrite => Some("上書き"),
        TextKey::ScanTextureLayer => Some("スキャンテクスチャ"),
        TextKey::PinPrompt => Some("それぞれにピンを置くか、ピンなしで進めます"),
        TextKey::TextureNeedsImage => Some("サイドパネルから画像を追加してください"),
        TextKey::TexturePinPairPrompt => Some("左右に対応するピンを打って合わせます"),
        TextKey::TextureToolNeedsPins => Some("先にピンを配置すると使えます"),
        TextKey::SymmetrySuggestion => Some("きれいに合わせるため左右対称をおすすめします"),
        TextKey::SymmetryChangeTitle => Some("ピンがリセットされます"),
        TextKey::SymmetryChangeConfirm => Some("確認"),
        TextKey::EyeClosure => Some("目を閉じる"),
        TextKey::VaMFolder => Some("VaM フォルダー"),
        #[cfg(test)]
        TextKey::VaMGeometryBaseReady => Some("VaM ベースの検証が完了しました"),
        TextKey::VaMGeometryBaseRejected => Some("VaM ベースを使用できません"),
        TextKey::VaMGeometryBaseHowTo => Some(
            "VaM フォルダーを指定してください。インストール済みの VaM からニュートラル G2 ベースが自動的に抽出されます",
        ),
        TextKey::VaMBaseSourceUnityBundle => Some("VaM 内蔵ニュートラルベース（自動抽出）"),
        TextKey::VaMBaseSourceDazAnchorPair => Some("DAZ アンカー + 検証済み OBJ ベース"),
        TextKey::VaMBaseSourceUnityAnchorPair => {
            Some("抽出ニュートラルアンカー + 検証済み OBJ ベース")
        }
        TextKey::VaMBaseSourceUnverifiedObj => Some("未検証の OBJ ベース"),
        TextKey::VaMUvMappingUnavailable => Some(
            "VaM スキンの UV マッピングがまだ準備できていません。VaM フォルダーを確認してスキン一覧を更新してください",
        ),
        TextKey::VaMIndexing => Some("VaM カタログを読み込み中"),
        TextKey::VaMFailed => Some("VaM カタログの読み込みに失敗しました"),
        TextKey::RefreshSkins => Some("スキン一覧を更新"),
        TextKey::MorphLoading => Some("モーフを読み込み中"),
        TextKey::MorphLoadFailed => Some("モーフの読み込みに失敗しました"),
        TextKey::DefaultSkinMissing => {
            Some("スキンファイルが見つかりません。VaM パスを確認してください")
        }
        TextKey::DefaultSkinSet => Some("起動時のスキンに設定"),
        TextKey::DefaultSkinClear => Some("起動時のスキン設定を解除"),
        TextKey::SkinNone => Some("なし"),
        TextKey::SkinSearch => Some("スキンを検索"),
        TextKey::SkinLoading => Some("スキンを読み込み中"),
        TextKey::SkinReady => Some("スキンプレビュー準備完了"),
        TextKey::SkinLoadFailed => Some("スキンの読み込みに失敗しました"),
        TextKey::SkinUvUnavailable => Some("スキン UV を使用できません"),
        TextKey::NoMatchingSkins => Some("一致するスキンがありません"),
        TextKey::HairSearch => Some("ヘアを検索"),
        TextKey::HairReady => Some("ヘアプレビューの準備完了"),
        TextKey::HairLoadFailed => Some("ヘアの読み込みに失敗しました"),
        TextKey::HairPartsSkipped => Some("一部のパーツを除いて表示"),
        TextKey::HairParts => Some("パーツ"),
        TextKey::BaseFace => Some("基本の顔"),
        TextKey::Settings => Some("設定"),
        TextKey::SettingsGraphics => Some("グラフィック"),
        TextKey::SettingsViewport => Some("ビューポート"),
        TextKey::SettingsGeneral => Some("一般"),
        TextKey::SettingsAbout => Some("情報"),
        TextKey::SettingsShortcuts => Some("ショートカット"),
        TextKey::ShortcutsCapturing => Some("押してください…"),
        TextKey::ShortcutsResetAll => Some("すべてのショートカットをリセット"),
        TextKey::ShortcutGroupSystem => Some("システム"),
        TextKey::ShortcutGroupAlignment => Some("位置合わせ"),
        TextKey::ShortcutGroupSculpt => Some("造形"),
        TextKey::ShortcutGroupHair => Some("ヘア"),
        TextKey::ShortcutsExport => Some("キーマップをファイルに保存"),
        TextKey::ShortcutsImport => Some("キーマップファイルを読み込む"),
        TextKey::ShortcutsTaken => Some("他の操作が既に使っています"),
        TextKey::SettingsAboutBuild => Some("ビルド"),
        TextKey::SettingsAboutLicense => Some("ライセンス"),
        TextKey::SettingsAboutDiagnostics => Some("診断"),
        TextKey::SettingsTooltipsTooltip => Some("ちょうどこんなふうに出るヘルプのことです"),
        TextKey::AboutTagline => Some("VaM のヘッドモーフを作るための補助ツールです。"),
        TextKey::AboutNoAffiliation => Some(
            "Virt-A-Mate の権利は MeshedVR に、Genesis 2 の権利は DAZ 3D にあります。Vkit はいずれとも無関係な独立したツールです。",
        ),
        TextKey::AboutNoWarranty => Some(
            "無保証・現状のまま提供されます。作ったものと、読み込んだコンテンツのライセンスは利用者の責任です。",
        ),
        TextKey::AboutSupport => Some("支援"),
        TextKey::AboutReportProblem => Some(
            "Vkit は起動のたびにログを残し、異常終了したときはその記録を別ファイルに書き出します。フォルダーを開き、両方を報告に添えてください。誰にも見えない不具合は直しようがありません。",
        ),
        TextKey::AboutOpenLogs => Some("ログフォルダーを開く"),
        TextKey::AboutReportIssue => Some("不具合を報告"),
        TextKey::SettingsLightingPreset => Some("プリセット"),
        TextKey::SettingsExposure => Some("明るさ"),
        TextKey::SettingsSmoothPasses => Some("サーフェススムージング"),
        TextKey::SettingsSmoothPassesHint => {
            Some("表示のみをVaMと同じ方式でなめらかにします。保存される形状は変わりません。")
        }
        TextKey::SettingsBackgroundGroup => Some("背景"),
        TextKey::SettingsBackground => Some("スタイル"),
        TextKey::SettingsOverlayGroup => Some("オーバーレイ"),
        TextKey::SettingsWireframeColor => Some("ワイヤーフレーム色"),
        TextKey::SettingsWireframeOpacity => Some("ワイヤーフレーム不透明度"),
        TextKey::SettingsXrayOpacity => Some("X-ray 不透明度"),
        TextKey::SettingsSolidColor => Some("単色ヘッドの色"),
        TextKey::SettingsLanguageGroup => Some("言語"),
        TextKey::SettingsMorphNames => Some("ビルトインモーフ表記"),
        TextKey::SettingsMorphNamesTranslated => Some("翻訳"),
        TextKey::SettingsMorphNamesOriginal => Some("英語の原名"),
        TextKey::SettingsLanguage => Some("表示言語"),
        TextKey::SettingsFoldersGroup => Some("フォルダー"),
        TextKey::SettingsVaMRoot => Some("VaM インストールパス"),
        TextKey::HairNeedsFinishedHead => Some("ヘアは完成したヘッドに着けます。"),
        TextKey::NoMatchingHair => Some("一致するヘアプリセットはありません"),
        TextKey::MorphSearch => Some("モーフを検索"),
        TextKey::MorphOneSidedFilter => Some("左/右"),
        TextKey::MorphOneSidedFilterShow => Some("片側だけ動くモーフを表示"),
        TextKey::MorphOneSidedFilterHide => Some("片側だけ動くモーフを隠す"),
        TextKey::AppearanceSearch => Some("外見を検索"),
        TextKey::AppearanceLayers => Some("外見レイヤー"),
        TextKey::AddAppearanceLayer => Some("選択した外見を追加"),
        TextKey::AppearanceLayersEmpty => Some("レイヤーなし"),
        TextKey::AppearanceLayerRaise => Some("上へ"),
        TextKey::AppearanceLayerLower => Some("下へ"),
        TextKey::LookFind => Some("外見を読み込む"),
        TextKey::CameraTrackballArmed => {
            Some("トラックボール · ドラッグで自由回転 · クリックまたは R で終了")
        }
        TextKey::HelpTrackball => Some("トラックボール回転"),
        TextKey::ShortcutTrackball => Some("R"),
        TextKey::SplitModelViewTooltip => Some("2 つの角度から見る — ビューを分割"),
        TextKey::SplitModelViewName => Some("分割ビュー"),
        TextKey::HelpLevelRoll => Some("傾き（ロール）をリセット"),
        TextKey::ShortcutLevelRoll => Some("Alt+R"),
        TextKey::RecoveryTitle => Some("前回のセッションが異常終了しました"),
        TextKey::RecoveryBody => Some(
            "終了直前のモーフ・スカルプト・テクスチャレイヤーが保存されています。復元するか選んでください。",
        ),
        TextKey::RecoveryRestore => Some("復元"),
        TextKey::RecoveryDiscard => Some("新規に開始"),
        TextKey::RecoveryRestored => Some("前回のセッションを復元しました。"),
        TextKey::RecoveryLookMissing => Some(
            "そのセッションの外見はまだ一覧にありません — テクスチャは復元済みで、編集内容は外見を選ぶと適用されます。",
        ),
        TextKey::SettingsBrushSweepGroup => Some("ブラシサイズ操作"),
        TextKey::BrushSweepBlender => Some("Blender"),
        TextKey::BrushSweepZBrush => Some("ZBrush"),
        TextKey::BrushSweepTooltip => {
            Some("Blenderはクリックか再押下で確定、ZBrushはキーを離すと確定します")
        }
        TextKey::HairPackageStyle => Some(".varで書き出し"),
        TextKey::HairPackageDone => Some("ヘア書き出し完了"),
        TextKey::HairPackageFailed => Some("書き出し失敗"),
        TextKey::HairPackageNeedsInstall => Some("先にヘアを書き出してください"),
        TextKey::DialogOpenHeadPreset => Some("ヘッドプリセットを開く"),
        TextKey::FindHeadPreset => Some("プリセットを探す…"),
        TextKey::SettingsVignetteIntensity => Some("強さ"),
        TextKey::SettingsVignetteSmoothness => Some("柔らかさ"),
        TextKey::SettingsVignetteRoundness => Some("丸み"),
        TextKey::SculptNotCarried => Some("スカルプトはこの外見へ引き継げませんでした"),
        TextKey::SettingsToneCurve => Some("トーンカーブ"),
        TextKey::SettingsToneCurveTooltip => Some("影とハイライトを調整します"),
        TextKey::SettingsVignetteTooltip => Some("画面の縁を暗くします"),
        TextKey::SettingsEffectEnabled => Some("有効"),
        TextKey::SettingsInterfaceGroup => Some("インターフェース"),
        TextKey::SettingsTooltips => Some("ツールチップを表示"),
        TextKey::SettingsResetGroup => Some("リセット"),
        TextKey::SettingsClearCache => Some("キャッシュを空にする"),
        TextKey::SettingsClearCacheTooltip => Some(
            "抽出済みのベースとモーフバンクです。必要になれば作り直されるので、空にしても次回の起動が一度遅くなるだけです。",
        ),
        TextKey::SettingsResetAll => Some("すべての設定を初期化して再起動"),
        TextKey::SettingsGraphicsLighting => Some("ライティング"),
        TextKey::SettingsGraphicsEffects => Some("エフェクト"),
        TextKey::SettingsGraphicsQuality => Some("品質"),
        TextKey::SettingsMsaa => Some("MSAA"),
        TextKey::SettingsMsaaTooltip => {
            Some("サンプル数の分だけ輪郭のギザギザをならす。高いほど滑らかで重い。")
        }
        TextKey::SoloViewHint => Some("このパーツだけ表示"),
        TextKey::SoloViewRestoreHint => Some("前の表示に戻す"),
        TextKey::ToneCurveFilmic => Some("フィルミック"),
        TextKey::ToneCurveSoft => Some("ソフト"),
        TextKey::LooksHiddenForOtherFigure => Some("別の性別の外見は非表示です"),
        TextKey::LookBelongsToOtherFigure => Some("別の性別向けの外見です"),
        TextKey::MorphEdit => Some("モーフ編集"),
        TextKey::VaMMorphPairImported => Some("VaM モーフを読み込みました"),
        TextKey::VaMMorphPairRejected => Some("VaM モーフを読み込めません"),
        TextKey::MorphCategoryAll => Some("すべて"),
        TextKey::MorphCategoryEyes => Some("目"),
        TextKey::MorphCategoryBrows => Some("眉"),
        TextKey::MorphCategoryNose => Some("鼻"),
        TextKey::MorphCategoryMouth => Some("口"),
        TextKey::MorphCategoryJaw => Some("顎"),
        TextKey::MorphCategoryCheeks => Some("頬"),
        TextKey::MorphCategoryEars => Some("耳"),
        TextKey::MorphCategoryHead => Some("顔"),
        TextKey::MorphCategoryExpression => Some("表情"),
        TextKey::NoMatchingMorphs => Some("一致するモーフがありません"),
        TextKey::ResetMorphs => Some("すべてリセット"),
        TextKey::UndoMorphReset => Some("リセットを取り消す"),
        TextKey::LightRotationGesture => Some("Shift+右ドラッグ"),
        TextKey::UpdateAvailable => Some("更新"),
        TextKey::PackageFromFiles => Some("ファイルを選ぶ"),
        TextKey::PackageMorphFiles => Some("モーフ"),
        TextKey::PackageTextureFiles => Some("テクスチャ"),
        TextKey::PackageListEmpty => Some("まだ追加されていません"),
        TextKey::LightingStudio => Some("スタジオ"),
        TextKey::LightingSoft => Some("ソフト"),
        TextKey::LightingDaylight => Some("昼光"),
        TextKey::LightingWarm => Some("ウォーム"),
        TextKey::LightingGloss => Some("光沢チェック"),
        TextKey::LightingPortrait => Some("ポートレート"),
        TextKey::ImportLoadingMesh => Some("メッシュを読み込み中"),
        TextKey::ImportSimplifying => Some("メッシュを単純化中"),
        TextKey::StageOptimizeHead => Some("カスタムヘッド最適化"),
        TextKey::StagePrepareInput => Some("入力準備"),
        TextKey::StageAlignScan => Some("スキャン位置合わせ"),
        TextKey::StageFitShape => Some("G2 形状フィッティング"),
        TextKey::StageValidate => Some("構造検証"),
        TextKey::StagePrepareResult => Some("結果準備"),
        TextKey::HeavyMeshNote => Some("約 {count} ポリゴン -- 多いほど時間がかかります"),
        TextKey::TransformGroups => Some("変形パーツ"),
        TextKey::CollapseTransformGroups => Some("パーツを折りたたむ"),
        TextKey::ExpandTransformGroups => Some("パーツを展開"),
        TextKey::Skin => Some("肌"),
        TextKey::SculptTear => Some("涙膜"),
        TextKey::SculptLips => Some("唇"),
        TextKey::SculptTeethTongue => Some("歯・舌"),
        TextKey::SculptInnerMouth => Some("口内"),
        TextKey::SculptEyes => Some("眼球"),
        TextKey::Size => Some("サイズ"),
        TextKey::SculptXSymmetryTooltip => Some("左右に同じ変形を適用します"),
        TextKey::BackfaceProtection => Some("裏面保護"),
        TextKey::AutoGroup => Some("オートグループ"),
        TextKey::AutoGroupTooltip => Some("グループ単位でのみ変形します"),
        TextKey::SculptXSymmetry => Some("X 対称"),
        TextKey::EyeTrackingAuto => Some("自動"),
        TextKey::EyeGazeFreeze => Some("モーフとして固定"),
        TextKey::EyeGazeFreezeTooltip => {
            Some("現在の視線をモーフとして固定し、保存タブでこの目のポーズを書き出します")
        }
        TextKey::Reset => Some("リセット"),
        TextKey::SculptGrab => Some("グラブ"),
        TextKey::SculptSmooth => Some("スムーズ"),
        TextKey::SculptPushPull => Some("押す・引く"),
        TextKey::ShortcutSculptGrab => Some("ドラッグ"),
        TextKey::ShortcutSculptSmooth => Some("Shift + ドラッグ"),
        TextKey::ShortcutSculptPushPull => Some("Ctrl / Alt + ドラッグ"),
        TextKey::ResetSculpt => Some("変形をリセット"),
        #[cfg(test)]
        TextKey::SculptApplied => Some("スカルプト編集を適用しました"),
        TextKey::SculptFailed => Some("スカルプト操作に失敗しました"),
        TextKey::Ready => Some("準備完了"),
        TextKey::Busy => Some("生成中は編集できません"),
        TextKey::NeedScan => Some(
            "先にスキャンヘッドの OBJ/GLB/FBX を読み込むか、VaM にあるルックから始めてください",
        ),
        TextKey::ScanLoadFailed => Some("このヘッドファイルを読み込めませんでした"),
        TextKey::NeedEyeMorph => Some("内蔵の目モーフは現在の G2 と互換性がありません"),
        TextKey::ReviewEyePins => Some("赤いまぶたピンを確認してください"),
        TextKey::ResultUnavailable => Some("先に顔フィッティングを行ってください"),
        TextKey::MorphNotInLibrary => {
            Some("このモーフはまだ一覧にありません。カタログを読み込み中です。")
        }
        TextKey::SculptNeedsBaseHead => Some("先に作成タブでベースヘッドで進んでください。"),
        TextKey::MorphUnavailable => Some("使用できる結果モーフがありません"),
        TextKey::AlignmentPending => {
            Some("スキャンヘッドを読み込み、VaM フォルダーを指定してください")
        }
        TextKey::ScanLoaded => Some("スキャンヘッドの OBJ/GLB/FBX を読み込みました"),
        TextKey::ScanUnloaded => Some("ヘッドファイルを外しました。G2 ベースを編集できます"),
        TextKey::PinPairsMismatched => Some("ピンの数が一致しません。相手のないピン番号"),
        TextKey::UnloadScanTooltip => Some("このヘッドファイルを外す"),
        TextKey::TemplateLoaded => Some("G2 を読み込みました"),
        TextKey::PinsReset => Some("すべてのピンをリセットしました"),
        TextKey::ResultStale => Some("変更があるため結果を再生成してください"),
        TextKey::GenerationStarted => Some("顔フィッティングを開始しました"),
        TextKey::Cancelling => Some("顔フィッティングをキャンセル中です"),
        TextKey::GenerationComplete => Some("顔フィッティング完了"),
        TextKey::GenerationCancelled => Some("顔フィッティングをキャンセルしました"),
        TextKey::GenerationFailed => Some("顔フィッティングに失敗しました"),
        TextKey::ExportStarted => Some("書き出し中"),
        TextKey::ExportComplete => Some("書き出し完了"),
        TextKey::ExportFailed => Some("書き出しに失敗しました"),
        TextKey::MorphNameHint => Some("モーフ名を入力してください"),
        TextKey::MorphGroupHint => Some("グループ （例: Custom, Morphs/Morph Loader）"),
        TextKey::MorphRegionHint => Some("領域 （例: Custom, Morphs/Body）"),
        TextKey::DirectEntry => Some("直接入力"),
        TextKey::VaMMetadataPoseMorph => Some("ポーズモーフとして登録"),
        TextKey::VaMMetadataPoseMorphTooltip => Some("表情やポーズと一緒に扱われます"),
        TextKey::VaMMetadataBoneCorrection => Some("ボーン補正"),
        TextKey::VaMMetadataBoneCorrectionTooltip => {
            Some("変形した骨格に合わせて中心軸を再設定します")
        }
        TextKey::NativeCorePending => Some("ネイティブフィッティングコアを統合中です"),
        TextKey::MorphPreviewUpdated => Some("モーフプレビューを更新しました"),
        TextKey::MorphsReset => Some("モーフをリセットしました"),
        TextKey::MorphsResetUndone => Some("リセットを元に戻しました"),
        TextKey::MorphEditUndone => Some("モーフ調整を元に戻しました"),
        TextKey::MorphApplied => Some("モーフを適用しました"),
        TextKey::HelpPlace => Some("ピンを配置"),
        TextKey::HelpMove => Some("ピンを移動"),
        TextKey::HelpDelete => Some("ピンを削除"),
        TextKey::HelpXSymmetry => Some("ピン X 対称"),
        TextKey::HelpSnapRotation => Some("回転スナップ"),
        TextKey::HelpDragZoom => Some("ドラッグズーム"),
        TextKey::HelpOrbit => Some("回転"),
        TextKey::HelpPan => Some("移動"),
        TextKey::HelpZoom => Some("ズーム"),
        TextKey::HelpLight => Some("ライト回転"),
        TextKey::HelpFrameView => Some("画面に合わせる"),
        TextKey::HelpUndo => Some("元に戻す"),
        TextKey::ShortcutPlace => Some("左クリック"),
        TextKey::ShortcutMove => Some("Alt + ドラッグ"),
        TextKey::ShortcutDelete => Some("右クリック"),
        TextKey::ShortcutXSymmetry => Some("X"),
        TextKey::ShortcutSnapRotation => Some("Shift / Ctrl + 回転ドラッグ"),
        TextKey::ShortcutDragZoom => Some("Ctrl + 中ボタン縦ドラッグ"),
        TextKey::ShortcutOrbit => Some("中ボタンドラッグ"),
        TextKey::ShortcutPan => Some("Shift + 中ボタンドラッグ"),
        TextKey::ShortcutZoom => Some("マウスホイール"),
        TextKey::ShortcutLight => Some("Shift + 右ドラッグ"),
        TextKey::ShortcutFrameView => Some("F"),
        TextKey::HelpSnapView => Some("スナップビュー"),
        TextKey::ShortcutSnapView => Some("Alt + ホイールドラッグ"),
        TextKey::HelpStandardViews => Some("標準ビュー"),
        TextKey::HelpCameraProjection => Some("遠近/平行 切替"),
        TextKey::ShortcutStandardViews => Some("テンキー 1-9"),
        TextKey::ShortcutCameraProjection => Some("テンキー ."),
        TextKey::ShortcutUndo => Some("Ctrl+Z"),
        TextKey::HelpRedo => Some("やり直す"),
        TextKey::ShortcutRedo => Some("Ctrl+Y"),
        TextKey::HairPresetSection => Some("ヘアプリセット"),
        TextKey::HairPresetLoad => Some("読み込む"),
        TextKey::HairToolPick => Some("レイヤーを選ぶ"),
        TextKey::HairToolPickHint => Some(
            "ビューポートで毛束をクリックすると、そのレイヤーが有効になります。ブラシは次のクリックから動きます。",
        ),
        TextKey::HelpHairPick => Some("カーソル下のレイヤーを選ぶ"),
        TextKey::ShortcutHairPick => Some("V、またはどのブラシでも Ctrl + クリック"),
        TextKey::HelpHairSmooth => Some("とかす代わりにならす"),
        TextKey::ShortcutHairSmooth => Some("とかしながら Shift"),
        TextKey::HairGroupScalp => Some("頭皮"),
        TextKey::HairScalpAlpha => Some("頭皮マスク"),
        TextKey::HairScalpPanel => Some("頭皮"),
        TextKey::HairScalpMesh => Some("頭皮メッシュ"),
        TextKey::HairScalpCreate => Some("頭皮を作成"),
        TextKey::HairScalpAbsent => Some("このスタイルにはまだ頭皮レイヤーがありません。"),
        TextKey::HistoryBranchTitle => Some("この先の作業履歴が失われます"),
        TextKey::HistoryBranchProceed => Some("続ける"),
        TextKey::DoNotShowAgain => Some("今後表示しない"),
        TextKey::SurfaceBaseUnblended => {
            Some("一部の PBR マップを VaM スキンと合成できませんでした")
        }
        TextKey::DetailEditRedone => Some("取り消した編集を再適用しました"),
        TextKey::ShortcutBrushSmaller => Some("ブラシを小さく"),
        TextKey::ShortcutBrushLarger => Some("ブラシを大きく"),
        TextKey::ShortcutBrushSizeDrag => Some("ブラシサイズ調整 (押しながら左右)"),
        TextKey::ShortcutBrushStrengthDrag => Some("ブラシ強度調整 (押しながら左右)"),
        TextKey::ShortcutStencilCancel => Some("ステンシル配置を取り消す"),
        TextKey::ShortcutViewOrbit => Some("ビューを軌道回転 (ドラッグ)"),
        TextKey::ShortcutViewPan => Some("ビューを平行移動 (ドラッグ)"),
        TextKey::ShortcutViewDolly => Some("ビューをドリー (ドラッグ)"),
        TextKey::TemplatePending => {
            Some("VaM フォルダーを指定すると G2 ベースが自動的に準備されます")
        }
        TextKey::ResultEmpty => Some("準備されたモーフがありません"),
        TextKey::ScaleLink => Some("スケール連動"),
        TextKey::ScaleLinkTooltip => Some("X/Y/Z のスケールをまとめて調整"),
        TextKey::CameraSettings => Some("カメラ設定"),
        TextKey::Perspective => Some("透視投影"),
        TextKey::Orthographic => Some("平行投影"),
        TextKey::Fov => Some("視野角"),
        TextKey::ResetCamera => Some("カメラをリセット"),
        TextKey::WindowMinimize => Some("最小化"),
        TextKey::WindowMaximize => Some("最大化"),
        TextKey::WindowRestore => Some("元のサイズ"),
        TextKey::WindowClose => Some("閉じる"),
        TextKey::TooltipShow => Some("表示"),
        TextKey::TooltipHide => Some("非表示"),
        TextKey::TooltipLock => Some("ロック"),
        TextKey::TooltipUnlock => Some("ロック解除"),
        TextKey::AdjustEyeGaze => Some("視線を調整"),
        TextKey::ScanLoading => Some("スキャンヘッドを読み込み中"),
        TextKey::TemplateLoading => Some("G2 テンプレートを読み込み中"),
        TextKey::ResultLoading => Some("結果メッシュを読み込み中"),
        TextKey::VaMMorphPairLoading => Some("VaM モーフペアを読み込み中"),
        TextKey::SystemLog => Some("システムログ"),
        TextKey::Strength => Some("強度"),
        TextKey::StrengthShort => Some("強度"),
        TextKey::SculptBrushMove => Some("グラブ"),
        TextKey::SculptBrushSmooth => Some("スムーズ"),
        TextKey::SculptBrushRestore => Some("復元"),
        TextKey::SculptBrushMoveTooltip => {
            Some("ポインターに沿って表面を引っ張ります。強度は追従の割合です")
        }
        TextKey::SculptBrushSmoothTooltip => Some("表面の凹凸をなめらかにします"),
        TextKey::SculptBrushRestoreTooltip => Some("ベース形状に復元します"),
        TextKey::SculptBrushMask => Some("マスク"),
        TextKey::SculptBrushMaskTooltip => {
            Some("選択中の外見レイヤーを削り、下のレイヤーを出します。Altで戻します")
        }
        TextKey::TextureLayers => Some("テクスチャレイヤー"),
        TextKey::PackageSection => Some("VAR 書き出し"),
        TextKey::PackageCreatorHint => Some("作者"),
        TextKey::PackageNameHint => Some("パッケージ名"),
        TextKey::PackageDescriptionHint => Some("説明"),
        TextKey::PackageCreditsHint => Some("クレジット"),
        TextKey::PackageInstructionsHint => Some("使い方"),
        TextKey::PackageLinkHint => Some("支援リンク"),
        TextKey::PackageSave => Some("VAR 保存"),
        TextKey::PackageFromThisHead => Some("現在のヘッドから作成"),
        TextKey::PackageAddMorphs => Some("モーフを追加"),
        TextKey::PackageAddTextures => Some("テクスチャを追加"),
        TextKey::PackageNeedsVamRestart => Some("VaM の再起動が必要です"),
        TextKey::PackageSavedTo => Some("保存先"),
        TextKey::FieldRequired => Some("（必須）"),
        TextKey::FieldOptional => Some("（任意）"),
        TextKey::PackageSaveTo => Some("保存先"),
        TextKey::PackageNothingToPack => {
            Some("入れるものがありません。現在のヘッドを有効にするかファイルを追加してください。")
        }
        TextKey::PackageVersionMarker => Some("バージョン"),
        TextKey::PackageCreatorRequired => Some("作者名は必須です。"),
        TextKey::PackageNameRequired => Some("パッケージ名は必須です。"),
        TextKey::PackageReplaceTitle => Some("同じ名前の VAR が既にあります"),
        TextKey::PackageReplaceBody => {
            Some("上書きしますか？ このバージョンを参照するシーンは新しい内容を読み込みます。")
        }
        TextKey::LicenseByTooltip => Some("原作者表示"),
        TextKey::LicenseBySaTooltip => Some("原作者表示 / 同一条件で再配布"),
        TextKey::LicenseByNdTooltip => Some("原作者表示 / 改変禁止"),
        TextKey::LicenseByNcTooltip => Some("原作者表示 / 営利利用禁止"),
        TextKey::LicenseByNcSaTooltip => Some("原作者表示 / 営利禁止 / 同一条件"),
        TextKey::LicenseByNcNdTooltip => Some("原作者表示 / 営利禁止 / 改変禁止"),
        TextKey::LicensePcEaTooltip => Some("支援者向け先行公開、再配布禁止"),
        TextKey::AddTextureLayer => Some("画像を追加"),
        TextKey::AddG2UvTextureLayer => Some("UVマップを追加"),
        TextKey::AddTextureLayerTooltip => Some("顔写真を配置してテクスチャとして編集します"),
        TextKey::AddG2UvTextureLayerTooltip => Some("G2 用の face テクスチャを追加して編集します"),
        TextKey::AddTextureLayerHint => {
            Some("＋ボタンで顔画像または G2 テクスチャを追加してください。")
        }
        TextKey::BakingTextures => Some("テクスチャをベイク中…"),
        TextKey::BakeTexturesForSave => Some("保存用テクスチャをベイク"),
        TextKey::TextureMetalRough => Some("Metallic / Roughness"),
        TextKey::TextureGlossSmooth => Some("Glossiness / Smoothness"),
        TextKey::LayerOpacity => Some("レイヤーの不透明度"),
        TextKey::PinOpacity => Some("ピンの不透明度"),
        TextKey::TransferPins => Some("ピンを転送"),
        TextKey::TransferPinsTooltip => Some("配置したピンを他のレイヤー画像に複製します。"),
        TextKey::DeleteLayer => Some("レイヤーを削除"),
        TextKey::TextureNormalStrength => Some("ノーマル強度"),
        TextKey::TextureInvertScalar => Some("スカラー値を反転"),
        TextKey::BlendNormal => Some("通常"),
        TextKey::BlendMultiply => Some("乗算"),
        TextKey::BlendScreen => Some("スクリーン"),
        TextKey::BlendOverlay => Some("オーバーレイ"),
        TextKey::MatchToneToLayerBelow => Some("下のレイヤーに色調を合わせる"),
        TextKey::ImageMirror => Some("画像左右対称"),
        TextKey::ImageMirrorOff => Some("しない"),
        TextKey::ImageMirrorToLeft => Some("左へ"),
        TextKey::ImageMirrorToRight => Some("右へ"),
        TextKey::ImageAdjustments => Some("画像補正"),
        TextKey::Exposure => Some("露光量"),
        TextKey::Contrast => Some("コントラスト"),
        TextKey::Saturation => Some("彩度"),
        TextKey::Hue => Some("色相"),
        TextKey::Temperature => Some("色温度"),
        TextKey::TextureMaskPreviewTooltip => Some("アクティブレイヤーのマスクを半透明の赤で表示"),
        TextKey::ClearLayerMask => Some("レイヤーマスクを消去"),
        TextKey::ResetSourceRetouch => Some("元画像のレタッチをリセット"),
        TextKey::CloneSampleHint => Some("Alt クリックで複製元を選択してください。"),
        TextKey::BakeBase => Some("ベイク設定"),
        TextKey::TransparentOverlay => Some("レイヤーのみベイク"),
        TextKey::CurrentSkinBake => Some("VaM スキンとベイク"),
        TextKey::TransparentOverlayTooltip => {
            Some("スキンプリセットを除き、レイヤー画像のみをベイクします")
        }
        TextKey::CurrentSkinBakeTooltip => Some("スキンプリセットを下地として一緒にベイクします"),
        TextKey::SkinPresetRequired => Some("先に肌の設定でスキンプリセットを選んでください"),
        TextKey::SkinSettings => Some("肌の設定"),
        TextKey::ScanHeadColor => Some("スキャンヘッドの色"),
        TextKey::G2HeadColor => Some("G2 ヘッドの色"),
        TextKey::MorphFilterAll => Some("すべて"),
        TextKey::MorphFilterActive => Some("使用中"),
        TextKey::BrushDodge => Some("覆い焼き"),
        TextKey::BrushBurn => Some("焼き込み"),
        TextKey::BrushSaturate => Some("彩度を上げる"),
        TextKey::BrushDesaturate => Some("彩度を下げる"),
        TextKey::ShowLayersOnly => Some("レイヤー画像のみ表示"),
        TextKey::ShowLayersOnlyTooltip => Some("VaMスキンを隠してレイヤー画像のみ表示します"),
        TextKey::MatchToneTooltip => Some("下のレイヤーに合わせて露出・彩度・色温度を調整します"),
        TextKey::ClearLayerMaskTooltip => Some("このレイヤーのマスクを消去します"),
        TextKey::TextureMetalRoughTooltip => Some("メタリック/ラフネス規格で保存します"),
        TextKey::TextureGlossSmoothTooltip => Some("VaMが使うグロス/スムースネス規格で保存します"),
        TextKey::TextureInvertScalarTooltip => {
            Some("ソースが逆の規格のとき値を反転します(ラフネス ↔ グロス)")
        }
        TextKey::BaseWithoutSkin => Some("レイヤー"),
        TextKey::BaseWithoutSkinTooltip => Some(
            "プリセットなしで、描いたレイヤーだけをソリッドの上に表示します。書き出すテクスチャは変わりません",
        ),
        TextKey::SolidColorTooltip => Some("テクスチャなしでソリッドカラーのみを表示します"),
        TextKey::TextureSkinTooltip => {
            Some("VaM スキンプリセットの上に描いたレイヤーを重ねて表示します")
        }
        TextKey::BoundaryFeather => Some("顔の境界をぼかす"),
        TextKey::BoundaryFeatherTooltip => Some(
            "ディフューズのみに適用されます。ノーマルやラフネスはアルファで合成できないため境界はそのまま残ります",
        ),
        TextKey::Baking => Some("ベイク中…"),
        TextKey::TextureToolPinPair => Some("ピンペア — 3D 顔と元画像の同じ位置にピンを配置"),
        TextKey::TextureToolMask => Some("マスク — 塗ると表示、Alt を押すと非表示"),
        TextKey::TextureToolClone => {
            Some("コピースタンプ — Alt クリックで複製元を選び、描画してコピー")
        }
        TextKey::TextureToolDodgeBurn => Some("覆い焼き/焼き込み — 明るくし、Alt を押すと暗くする"),
        TextKey::TextureToolSponge => Some("スポンジ — 彩度を上げ、Alt を押すと下げる"),
        TextKey::SelectTextureLayer => Some("テクスチャレイヤーを選択してください。"),
        TextKey::LoadingTextureImage => Some("画像を読み込み中…"),
        TextKey::ScanTextureAlignment => {
            Some("スキャン表面テクスチャは調整済みの 3D 配置を使用します。")
        }
        TextKey::NoTextureImage => Some("画像がありません。"),
        TextKey::MorphSavedReloadFemale => {
            Some("VaM の Female Morphs タブで Reload Custom Morphs を押すと更新されます。")
        }
        TextKey::MorphSavedReloadMale => {
            Some("VaM の Male Morphs タブで Reload Custom Morphs を押すと更新されます。")
        }
        TextKey::BootStarting => Some("Vkit を準備中"),
        TextKey::BootFonts => Some("フォントを準備中"),
        TextKey::BootGraphics => Some("グラフィックスを初期化中"),
        TextKey::BootSettings => Some("設定を読み込み中"),
        TextKey::BootTemplate => Some("G2 を読み込み中"),
        TextKey::BootWorkspace => Some("ワークスペースを準備中"),
        TextKey::BootReady => Some("起動中"),
        TextKey::StartupFailedTitle => Some("Vkit を起動できませんでした"),
        TextKey::StartupFailedGraphics => Some(
            "Vkit は DirectX 12 で描画しますが、このマシンのグラフィックスはそれを提供できませんでした。グラフィックスドライバーを更新するか、DirectX 12 対応の GPU を搭載したマシンで Vkit を実行してください。リモートデスクトップセッションや仮想マシンには、自前の DirectX 12 ディスプレイがないことがよくあります。",
        ),
        TextKey::StartupFailedOther => {
            Some("Vkit は起動の途中で停止し、ウィンドウを開けませんでした。")
        }
        TextKey::StartupFailedLogHint => Some("何が起きたかは次のファイルに記録しました:"),
        TextKey::DialogOpenScan => Some("頭部の OBJ / GLB / FBX を開く"),
        TextKey::DialogAddUvLayer => Some("G2 UV テクスチャレイヤーを追加"),
        TextKey::DialogAddTextureLayer => Some("テクスチャレイヤーを追加"),
        TextKey::DialogSaveMorphPair => Some("VaM モーフ VMI + VMB を保存"),
        TextKey::DialogChooseVamFolder => Some("Virt-A-Mate フォルダーを選択"),
        _ => None,
    }
}

pub(super) fn hair_label(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some("髪の量"),
        "curveDensity" => Some("カーブ密度"),
        "iterations" => Some("反復回数"),
        "simulationEnabled" => Some("物理"),
        "collisionEnabled" => Some("衝突"),
        "weight" => Some("重さ"),
        "drag" => Some("空気抵抗"),
        "gravityMultiplier" => Some("重力倍率"),
        "collisionRadius" => Some("衝突半径"),
        "collisionRadiusRoot" => Some("衝突半径(根元)"),
        "friction" => Some("摩擦"),
        "bendResistance" => Some("曲げ抵抗"),
        "rootRigidity" => Some("根元の弾性"),
        "mainRigidity" => Some("中間の弾性"),
        "tipRigidity" => Some("毛先の弾性"),
        "jointRigidity" => Some("ジョイント弾性"),
        "rigidityRolloffPower" => Some("弾性の減衰"),
        "cling" => Some("スタイル凝集"),
        "clingRolloff" => Some("凝集の減衰"),
        "snap" => Some("スナップ"),
        "usePaintedRigidity" => Some("塗った弾性を使う"),
        "width" => Some("太さ"),
        "length1" => Some("長さ 1"),
        "length2" => Some("長さ 2"),
        "length3" => Some("長さ 3"),
        "maxSpread" => Some("最大広がり"),
        "spreadRoot" => Some("広がり(根元)"),
        "spreadMid" => Some("広がり(中間)"),
        "spreadTip" => Some("広がり(毛先)"),
        "spreadMidpoint" => Some("広がりの中点"),
        "spreadCurvePower" => Some("広がりカーブ"),
        "curlScale" => Some("カールの大きさ"),
        "curlFrequency" => Some("カールの頻度"),
        "curlScaleRandomness" => Some("カール大きさランダム"),
        "curlFrequencyRandomness" => Some("カール頻度ランダム"),
        "curlRoot" => Some("カール(根元)"),
        "curlMid" => Some("カール(中間)"),
        "curlTip" => Some("カール(毛先)"),
        "curlMidpoint" => Some("カールの中点"),
        "curlCurvePower" => Some("カールカーブ"),
        "curlX" => Some("カール X"),
        "curlY" => Some("カール Y"),
        "curlZ" => Some("カール Z"),
        "curlNormalAdjust" => Some("カール法線補正"),
        "curlAllowReverse" => Some("逆巻きを許可"),
        "curlAllowFlipAxis" => Some("軸反転を許可"),
        "Alpha Adjust" => Some("頭皮の不透明度"),
        "Diffuse Color" => Some("頭皮の拡散色"),
        "Gloss" => Some("頭皮の光沢"),
        "Specular Intensity" => Some("頭皮の反射"),
        "Global Illumination Filter" => Some("頭皮の環境光"),
        "Diffuse Texture Offset" => Some("頭皮テクスチャオフセット"),
        "rootColor" => Some("根元の色"),
        "tipColor" => Some("毛先の色"),
        "specularColor" => Some("光沢の色"),
        "colorRolloff" => Some("根元→毛先の色減衰"),
        "randomColorPower" => Some("ランダム色の強さ"),
        "randomColorOffset" => Some("ランダム色のオフセット"),
        "primarySpecularSharpness" => Some("一次ハイライト"),
        "secondarySpecularSharpness" => Some("二次ハイライト"),
        "specularShift" => Some("二次ハイライトのずれ"),
        "diffuseSoftness" => Some("拡散光の柔らかさ"),
        "fresnelPower" => Some("フレネル強度"),
        "fresnelAttenuation" => Some("フレネル減衰"),
        "IBLFactor" => Some("間接光"),
        "normalRandomize" => Some("法線ランダム"),
        _ => None,
    }
}

pub(super) fn hair_hint(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => {
            Some("ガイド三本の間に何本生えるか。髪の印象そのものであり、負荷の大半でもあります。")
        }
        "curveDensity" => Some(
            "毛束一本に打つ点の数。多いほど滑らかで重くなります。形は数が増える前に読み取れます。",
        ),
        "iterations" => Some("フレームあたりのソルバー反復。多いほど速い動きでも拘束を保ちます。"),
        "simulationEnabled" => {
            Some("このパーツがゲーム内で動くか。ビューポートのスイッチとは無関係です。")
        }
        "collisionEnabled" => Some("このパーツの毛束が体の外へ押し出されるか。"),
        "weight" => Some("毛束がどれだけ重く垂れるか。重力と、動かされにくさを同時に上げます。"),
        "drag" => Some("空気が毛束をどれだけ引き止めるか。高いと早く収まり、あまり揺れません。"),
        "gravityMultiplier" => Some("このパーツにかかる重力。シーンの重力に対する倍率です。"),
        "collisionRadius" => Some("毛束が衝突する太さ。長さ方向の全体に効きます。"),
        "collisionRadiusRoot" => Some("根元付近の太さ。ほかと変えたいときに使います。"),
        "friction" => Some("触れたものを滑らずに掴む度合い。"),
        "bendResistance" => Some("まっすぐな状態から曲げられることへの抵抗。"),
        "rootRigidity" => Some("根元側がスタイリングされた形をどれだけ保つか。"),
        "mainRigidity" => Some("中間がスタイリングされた形をどれだけ保つか。"),
        "tipRigidity" => Some("毛先がスタイリングされた形をどれだけ保つか。"),
        "jointRigidity" => {
            Some("スタイルジョイントの硬さ。ポニーテールをポニーテールに保つバネです。")
        }
        "rigidityRolloffPower" => {
            Some("三つの弾性が毛束に沿ってどう混ざるか。高いほど根元の値が先まで届きます。")
        }
        "cling" => Some("スタイルジョイントが毛束同士を引き寄せる強さ。"),
        "clingRolloff" => {
            Some("凝集が毛束に沿って弱まる距離。高いほど根元寄りに閉じ込められます。")
        }
        "snap" => Some("各節が作成時の長さをどれだけ固く保つか。"),
        "usePaintedRigidity" => Some("三つのスライダーではなく、塗ったマップから弾性を読みます。"),
        "width" => Some("毛束を描く太さ。VaM のスライダーも 0.1mm で上限です。"),
        "length1" => Some("三角形の一つ目のガイドに近い毛束の長さ。"),
        "length2" => Some("三角形の二つ目のガイドに近い毛束の長さ。"),
        "length3" => Some("三角形の三つ目のガイドに近い毛束の長さ。"),
        "maxSpread" => Some(
            "毛束がガイドから離れられる距離の上限で、増幅ではありません。ガイド間隔を超えると、上げても何も変わりません。",
        ),
        "spreadRoot" => Some("根元で毛束がどれだけ自由に離れるか。低いとガイドへ寄せ集まります。"),
        "spreadMid" => Some("中間で毛束がどれだけ自由に離れるか。"),
        "spreadTip" => Some("毛先で毛束がどれだけ自由に離れるか。"),
        "spreadMidpoint" => Some("毛束のどこから中間の広がりが効くか。"),
        "spreadCurvePower" => Some("根元・中間・毛先の間で広がりが切り替わる急峻さ。"),
        "curlScale" => Some("カールの幅。巻く半径であって長さではありません。"),
        "curlFrequency" => Some("毛束 1cm あたりの巻き数。"),
        "curlScaleRandomness" => Some("毛束ごとにカール幅がどれだけばらつくか。"),
        "curlFrequencyRandomness" => Some("毛束ごとに巻く速さがどれだけばらつくか。"),
        "curlRoot" => Some("根元にカールがどれだけ届くか。"),
        "curlMid" => Some("中間にカールがどれだけ届くか。"),
        "curlTip" => Some("毛先にカールがどれだけ届くか。"),
        "curlMidpoint" => Some("毛束のどこから中間のカールが効くか。"),
        "curlCurvePower" => Some("根元・中間・毛先の間でカールが切り替わる急峻さ。"),
        "curlX" => {
            Some("カールの X 方向の変位。回転される ベクトルであって、巻く軸ではありません。")
        }
        "curlY" => Some("カールの Y 方向の変位。"),
        "curlZ" => Some("カールの Z 方向の変位。"),
        "curlNormalAdjust" => Some(
            "カールのオフセットに法線方向へ加算されます。軸を傾けるのではなく、コイルを頭から浮かせます。",
        ),
        "curlAllowReverse" => {
            Some("一部の毛束を逆巻きにします。巻き髪全体が一本の繰り返しに見えないように。")
        }
        "curlAllowFlipAxis" => Some("一部の毛束を反転した軸で巻かせます。理由は同じです。"),
        "Alpha Adjust" => Some(
            "頭皮キャップの不透明度。多くのパーツは他人のキャップを被るため最下段に置いて隠し、分け目で頭皮を見せたいときだけ上げます。",
        ),
        "Diffuse Color" => Some(
            "キャップ自体の拡散色。VaM は白で出荷し、そうすることで下のスキンシートが透けます。",
        ),
        "Gloss" => Some("頭皮キャップが見える箇所での光沢の強さ。"),
        "Specular Intensity" => Some("頭皮キャップがハイライトをどれだけ強く受けるか。"),
        "Global Illumination Filter" => Some("キャップが環境光をどれだけ取り込むか。"),
        "Diffuse Texture Offset" => {
            Some("キャップの拡散色に加算される値。名前に反してシート番号ではありません。")
        }
        "rootColor" => Some("根元側の色。"),
        "tipColor" => Some("毛先側の色。"),
        "specularColor" => Some("ハイライトの色。"),
        "colorRolloff" => Some("毛束に沿って根元の色が毛先の色へ移る具合。"),
        "randomColorPower" => Some("毛束どうしで色がどれだけばらつくか。"),
        "randomColorOffset" => Some("そのばらつきが明るい側か暗い側か。"),
        "primarySpecularSharpness" => Some("主ハイライトの締まり。高いほど艶やかです。"),
        "secondarySpecularSharpness" => {
            Some("二次ハイライトの締まり。髪に沿って動く色付きの方です。")
        }
        "specularShift" => Some("二次ハイライトが一次から毛束に沿ってどれだけ離れて座るか。"),
        "diffuseSoftness" => {
            Some("光が毛束を回り込む柔らかさ。低いと光の当たる側だけが明るくなります。")
        }
        "fresnelPower" => Some("髪が視線から逸れる縁がどれだけ急に明るくなるか。"),
        "fresnelAttenuation" => Some("その縁の明るさの強さ。"),
        "IBLFactor" => Some("シーンの環境光を髪がどれだけ受けるか。"),
        "normalRandomize" => {
            Some("毛束ごとに陰影法線をどれだけ揺らすか。隣同士が同じに陰らないようにします。")
        }
        _ => None,
    }
}
