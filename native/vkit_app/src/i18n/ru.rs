use super::TextKey;

pub(super) const fn label(key: TextKey) -> Option<&'static str> {
    #[allow(unreachable_patterns)]
    match key {
        TextKey::SurfaceSmooth => Some("Сглаживание меша"),
        TextKey::SurfaceSmoothTooltip => Some(
            "Сглаживание при отрисовке, совместимое с VaM. 0 — редактируемый базовый меш, 3 — значение VaM по умолчанию, 4 — максимум. Морфы, UV, скульптинг и запекание текстур сохраняют базовую топологию",
        ),
        TextKey::FaceMatch => Some("Создать"),
        TextKey::ResultPreview => Some("Сохранить"),
        TextKey::Save => Some("Сохранить"),
        TextKey::DetailCorrection => Some("Скульптинг"),
        TextKey::TextureStage => Some("Текстура"),
        TextKey::MorphCategoryBody => Some("Тело"),
        TextKey::TextureNamePlaceholder => Some("Введите имя текстуры"),
        TextKey::TextureOverwriteTitle => Some("Текстуры с таким же именем будут заменены"),
        TextKey::OpenScan => Some("Скан головы"),
        TextKey::SourceMorphMissing => Some("Ни один морф этого лука не установлен"),
        TextKey::EditDetails => Some("Правка деталей"),
        TextKey::Female => Some("Женский"),
        TextKey::Male => Some("Мужской"),
        TextKey::Lighting => Some("Освещение"),
        TextKey::Brightness => Some("Яркость"),
        TextKey::LightRotation => Some("Поворот света"),
        TextKey::AddHeadFileHint => Some("или перетащите сюда файл OBJ, GLB или FBX"),
        TextKey::AddHeadFile => Some("Добавить файл головы"),
        TextKey::AutoAlign => Some("Автоподгонка"),
        TextKey::PlacementReset => Some("Сброс размещения"),
        TextKey::Scale => Some("Масштаб"),
        TextKey::Position => Some("Позиция"),
        TextKey::Rotation => Some("Вращение"),
        TextKey::XMirror => Some("Зеркало X"),
        TextKey::XMirrorTooltip => {
            Some("При установке точки её пара ставится симметрично с другой стороны")
        }
        TextKey::NumbersTooltip => Some("Нумерует точки в порядке их расстановки"),
        TextKey::XMirrorRejected => Some("Зеркало X отключено: безопасная парная точка не найдена"),
        TextKey::ScanFidelity => Some("Влияние скана"),
        TextKey::ScanFidelityTooltip => Some(
            "При значениях выше 1 подгонка сильнее следует форме скана и меньше сохраняет гладкость фигуры. До 2 этим всё и ограничивается; после 2 шум скана — крапины, дыры, швы сшивки — тоже начинает проступать буграми, и на чистом плотном скане на это бывает смысл пойти. Точки не приносятся в жертву ни при каких значениях, и ни одна защита не ослабляется.",
        ),
        TextKey::ScanSymmetry => Some("Симметрия скана"),
        TextKey::Original => Some("Оригинал"),
        TextKey::Left => Some("Левая"),
        TextKey::Right => Some("Правая"),
        TextKey::EyesOpen => Some("Глаза открыты"),
        TextKey::EyesClosed => Some("Глаза закрыты"),
        TextKey::View => Some("Вид"),
        TextKey::On => Some("Вкл"),
        TextKey::Numbers => Some("Номера"),
        TextKey::PinPairsPlaced => Some("Точки"),
        TextKey::PinPlacement => Some("Расстановка точек"),
        TextKey::ScanReference => Some("Референс скана"),
        TextKey::AddHeadFileAction => Some("Добавить файл головы"),
        TextKey::AddHeadFileActionTooltip => {
            Some("Загрузить файл отсканированной головы (OBJ · GLB · FBX)")
        }
        TextKey::ContinueStep => Some("Продолжить с базовой головой"),
        TextKey::ContinueStepTooltip => Some(
            "Редактировать голову G2 как есть, без скана; скан можно загрузить и подогнать позже",
        ),
        TextKey::Undo => Some("Отменить"),
        TextKey::ResetAllPins => Some("Сбросить все точки"),
        TextKey::MorphSave => Some("Сохранить морф"),
        TextKey::TextureSaveSection => Some("Сохранить текстуры"),
        TextKey::Generate => Some("Подогнать лицо"),
        TextKey::Cancel => Some("Отмена"),
        TextKey::CopyAll => Some("Копировать всё"),
        TextKey::Next => Some("Сохранить"),
        TextKey::TextureSkin => Some("Пресет"),
        TextKey::SolidColor => Some("Сплошной"),
        TextKey::BackfaceProtectionTooltip => Some("Защищает задние грани тонких мешей"),
        TextKey::WireframeColor => Some("Цвет каркаса"),
        TextKey::WireframeSettings => Some("Каркас"),
        TextKey::XraySettings => Some("Рентген"),
        TextKey::ViewportLightingTooltip => Some("Настроить освещение"),
        TextKey::Background => Some("Фон"),
        TextKey::ViewportBackgroundTooltip => Some("Настроить фон и референсное изображение"),
        TextKey::BackgroundRadial => Some("Радиальный"),
        TextKey::BackgroundVertical => Some("Вертикальный"),
        TextKey::BackgroundFlat => Some("Однотонный"),
        TextKey::ViewportWireframeTooltip => Some("Показать и настроить наложение каркаса"),
        TextKey::ViewportXrayTooltip => Some("Показать и настроить рентген-наложение"),
        TextKey::ViewportSkinTooltip => Some("Выбрать текстуру кожи VaM"),
        TextKey::ViewportHairTooltip => Some("Выбрать пресет волос VaM"),
        TextKey::FalloffTooltip => Some("Выбрать кривую влияния кисти"),
        TextKey::FalloffSmooth => Some("Плавный"),
        TextKey::FalloffSmoother => Some("Плавнее"),
        TextKey::FalloffSharp => Some("Резкий"),
        TextKey::FalloffLinear => Some("Линейный"),
        TextKey::PlaceHead => Some("Разместить голову"),
        TextKey::PlaceHeadTooltip => Some(
            "Накладывает загруженную голову на G2, чтобы согласовать положение, поворот и размер.",
        ),
        TextKey::ScanOverlay => Some("Наложение скана"),
        TextKey::OverlayDone => Some("Завершить наложение"),
        TextKey::ProjectImage => Some("Проецировать"),
        TextKey::ProjectionCanvasHint => Some("Рисуйте в 3D-виде — мазки ложатся на этот UV-холст"),
        TextKey::HelpProjectionPaint => Some("Рисование (проекция)"),
        TextKey::ShortcutProjectionPaint => Some("Тянуть ЛКМ"),
        TextKey::HelpProjectionMove => Some("Двигать изображение"),
        TextKey::ShortcutProjectionMove => Some("Тянуть ПКМ"),
        TextKey::HelpProjectionRotate => Some("Вращать изображение"),
        TextKey::ShortcutProjectionRotate => Some("Shift + тянуть ПКМ"),
        TextKey::HelpProjectionZoom => Some("Масштаб изображения"),
        TextKey::ShortcutProjectionZoom => Some("Колесо"),
        TextKey::HelpProjectionBrushSize => Some("Размер кисти"),
        TextKey::ShortcutProjectionBrushSize => Some("F + тянуть · [ ]"),
        TextKey::HelpProjectionDone => Some("Завершить проекцию"),
        TextKey::ShortcutProjectionDone => Some("Готово · Esc"),
        TextKey::ProjectDone => Some("Готово"),
        TextKey::ProjectDoneTooltip => {
            Some("Завершает проецирование; всё нарисованное остаётся на слое")
        }
        TextKey::MorphCompare => Some("Сравнить"),
        TextKey::Opacity => Some("Непрозрачность"),
        TextKey::G2Head => Some("Голова G2"),
        TextKey::Eyelashes => Some("Ресницы"),
        TextKey::OverwriteTitle => Some("Перезаписать файл"),
        TextKey::OverwriteBody => Some("Файл с таким именем уже существует. Перезаписать?"),
        TextKey::Overwrite => Some("Перезаписать"),
        TextKey::ScanTextureLayer => Some("Текстура скана"),
        TextKey::PinPrompt => Some("Поставьте по метке на каждом или обойдитесь без них"),
        TextKey::TextureNeedsImage => Some("Добавьте изображение на боковой панели"),
        TextKey::TexturePinPairPrompt => Some("Расставьте парные точки слева и справа"),
        TextKey::SymmetrySuggestion => {
            Some("Для более чистого совпадения рекомендуется зеркалирование")
        }
        TextKey::SymmetryChangeTitle => Some("Точки будут сброшены"),
        TextKey::SymmetryChangeConfirm => Some("Подтвердить"),
        TextKey::EyeClosure => Some("Закрыть глаза"),
        TextKey::VaMFolder => Some("Папка VaM"),
        #[cfg(test)]
        TextKey::VaMGeometryBaseReady => Some("База геометрии VaM проверена"),
        TextKey::VaMGeometryBaseRejected => Some("База геометрии VaM непригодна"),
        TextKey::VaMGeometryBaseHowTo => Some(
            "Выберите папку VaM; нейтральная база G2 будет извлечена из вашей установки автоматически",
        ),
        TextKey::VaMBaseSourceUnityBundle => {
            Some("Встроенная нейтральная база VaM (извлечена автоматически)")
        }
        TextKey::VaMBaseSourceDazAnchorPair => Some("Якорь DAZ + проверенная база OBJ"),
        TextKey::VaMBaseSourceUnityAnchorPair => {
            Some("Извлечённый нейтральный якорь + проверенная база OBJ")
        }
        TextKey::VaMBaseSourceUnverifiedObj => Some("Непроверенная база OBJ"),
        TextKey::VaMUvMappingUnavailable => {
            Some("UV-развёртка кожи VaM не готова. Проверьте папку VaM и обновите каталог кожи")
        }
        TextKey::VaMIndexing => Some("Индексация каталога VaM"),
        TextKey::VaMFailed => Some("Сбой каталога VaM"),
        TextKey::RefreshSkins => Some("Обновить каталог кожи"),
        TextKey::MorphLoading => Some("Загрузка морфа"),
        TextKey::MorphLoadFailed => Some("Не удалось загрузить морф"),
        TextKey::DefaultSkinMissing => Some("Эта кожа не найдена. Проверьте папку VaM"),
        TextKey::DefaultSkinSet => Some("Открывать с этой кожей"),
        TextKey::DefaultSkinClear => Some("Больше не открывать с этой кожей"),
        TextKey::SkinNone => Some("Нет"),
        TextKey::SkinSearch => Some("Поиск кожи"),
        TextKey::SkinLoading => Some("Загрузка кожи"),
        TextKey::SkinReady => Some("Предпросмотр кожи готов"),
        TextKey::SkinLoadFailed => Some("Не удалось загрузить кожу"),
        TextKey::SkinUvUnavailable => Some("UV кожи недоступна"),
        TextKey::NoMatchingSkins => Some("Подходящей кожи нет"),
        TextKey::Hair => Some("Волосы"),
        TextKey::HairNone => Some("Нет"),
        TextKey::HairSearch => Some("Поиск волос"),
        TextKey::HairLoading => Some("Загрузка волос"),
        TextKey::HairReady => Some("Предпросмотр волос готов"),
        TextKey::HairLoadFailed => Some("Не удалось загрузить волосы"),
        TextKey::HairPartsSkipped => Some("Показано без некоторых частей"),
        TextKey::HairParts => Some("част."),
        TextKey::BaseFace => Some("Базовое лицо"),
        TextKey::Settings => Some("Настройки"),
        TextKey::SettingsGraphics => Some("Графика"),
        TextKey::SettingsViewport => Some("Вьюпорт"),
        TextKey::SettingsGeneral => Some("Общие"),
        TextKey::SettingsAbout => Some("О программе"),
        TextKey::SettingsAboutBuild => Some("Сборка"),
        TextKey::SettingsAboutLicense => Some("Лицензия"),
        TextKey::SettingsAboutDiagnostics => Some("Диагностика"),
        TextKey::SettingsTooltipsTooltip => {
            Some("Подсказки вроде этой, которая только что появилась")
        }
        TextKey::AboutTagline => Some("Вспомогательный инструмент для создания морфов головы VaM."),
        TextKey::AboutNoAffiliation => Some(
            "Права на Virt-A-Mate принадлежат MeshedVR, права на Genesis 2 — DAZ 3D. Vkit — независимый инструмент, не связанный ни с одной из них.",
        ),
        TextKey::AboutNoWarranty => Some(
            "Предоставляется как есть, без гарантий. Созданное вами и лицензия загружаемых материалов — на вашей ответственности.",
        ),
        TextKey::AboutSupport => Some("Поддержать"),
        TextKey::AboutReportProblem => Some(
            "Vkit пишет журнал при каждом запуске и отдельный файл, если падает. Откройте папку и приложите оба к сообщению: сбой, которого никто больше не видит, — это сбой, который некому исправить.",
        ),
        TextKey::AboutOpenLogs => Some("Открыть папку журналов"),
        TextKey::AboutReportIssue => Some("Сообщить о проблеме"),
        TextKey::SettingsLightingGroup => Some("Освещение"),
        TextKey::SettingsLightingPreset => Some("Пресет"),
        TextKey::SettingsExposure => Some("Экспозиция"),
        TextKey::SettingsGeometryGroup => Some("Геометрия"),
        TextKey::SettingsSmoothPasses => Some("Сглаживание поверхности"),
        TextKey::SettingsSmoothPassesHint => Some(
            "Сглаживает только отображение, как это делает VaM. Сохраняемая форма не меняется.",
        ),
        TextKey::SettingsBackgroundGroup => Some("Фон"),
        TextKey::SettingsBackground => Some("Стиль"),
        TextKey::SettingsOverlayGroup => Some("Наложения"),
        TextKey::SettingsWireframeColor => Some("Цвет каркаса"),
        TextKey::SettingsWireframeOpacity => Some("Непрозрачность каркаса"),
        TextKey::SettingsXrayOpacity => Some("Непрозрачность рентгена"),
        TextKey::SettingsSolidColor => Some("Сплошной цвет головы"),
        TextKey::SettingsLanguageGroup => Some("Язык"),
        TextKey::SettingsMorphNames => Some("Названия встроенных морфов"),
        TextKey::SettingsMorphNamesTranslated => Some("Перевод"),
        TextKey::SettingsMorphNamesOriginal => Some("Оригинал (англ.)"),
        TextKey::SettingsLanguage => Some("Язык интерфейса"),
        TextKey::SettingsFoldersGroup => Some("Папки"),
        TextKey::SettingsVaMRoot => Some("Путь установки VaM"),
        TextKey::HairVisible => Some("Показывать волосы"),
        TextKey::HairNeedsFinishedHead => Some("Волосы надеваются на готовую голову."),
        TextKey::HairPreviewCaveat => Some("Детализация волос отличается от настоящей."),
        TextKey::NoMatchingHair => Some("Подходящих пресетов волос нет"),
        TextKey::MorphSearch => Some("Поиск морфов"),
        TextKey::MorphOneSidedFilter => Some("Лево / право"),
        TextKey::MorphOneSidedFilterHint => {
            Some("Показывать морфы, изменяющие только одну половину лица")
        }
        TextKey::AppearanceSearch => Some("Поиск образов"),
        TextKey::LookFind => Some("Загрузить образ"),
        TextKey::CameraTrackballArmed => {
            Some("Вращение трекболом · наклон от края · щелчок или R для выхода")
        }
        TextKey::HelpTrackball => Some("Вращение трекболом"),
        TextKey::ShortcutTrackball => Some("R"),
        TextKey::RecoveryTitle => Some("Прошлый сеанс завершился неожиданно"),
        TextKey::RecoveryBody => Some(
            "Ваши морфы, скульптинг и слои текстур были сохранены прямо перед остановкой. Восстановить их?",
        ),
        TextKey::RecoveryRestore => Some("Восстановить"),
        TextKey::RecoveryDiscard => Some("Начать заново"),
        TextKey::RecoveryRestored => Some("Предыдущий сеанс восстановлен."),
        TextKey::RecoveryLookMissing => {
            Some("Образ того сеанса больше не установлен, поэтому восстановить ничего не удалось.")
        }
        TextKey::SettingsOcclusionGroup => Some("Затенение (AO)"),
        TextKey::SettingsOcclusionIntensity => Some("Сила"),
        TextKey::SettingsOcclusionRadius => Some("Охват"),
        TextKey::SettingsBloomGroup => Some("Свечение"),
        TextKey::SettingsBloomIntensity => Some("Интенсивность"),
        TextKey::SettingsBloomThreshold => Some("Порог"),
        TextKey::SettingsBloomSoftKnee => Some("Мягкий переход"),
        TextKey::SettingsBloomRadius => Some("Радиус"),
        TextKey::SettingsVignetteGroup => Some("Виньетка"),
        TextKey::SettingsVignetteIntensity => Some("Величина"),
        TextKey::SettingsVignetteSmoothness => Some("Мягкость"),
        TextKey::SettingsVignetteRoundness => Some("Округлость"),
        TextKey::SculptNotCarried => Some("Скульптинг не удалось перенести на этот образ"),
        TextKey::SettingsToneCurve => Some("Тоновая кривая"),
        TextKey::SettingsToneCurveTooltip => Some("Настраивает тени и света"),
        TextKey::SettingsOcclusionTooltip => Some("Затеняет складки в местах стыка поверхностей"),
        TextKey::SettingsVignetteTooltip => Some("Затемняет края кадра"),
        TextKey::SettingsEffectEnabled => Some("Включено"),
        TextKey::SettingsInterfaceGroup => Some("Интерфейс"),
        TextKey::SettingsTooltips => Some("Показывать подсказки"),
        TextKey::SettingsResetGroup => Some("Сброс"),
        TextKey::SettingsClearCache => Some("Очистить кэш"),
        TextKey::SettingsClearCacheTooltip => Some(
            "Извлечённые базы и банки морфов, которые создаются заново по мере надобности. Очистка стоит одного медленного запуска.",
        ),
        TextKey::SettingsResetAll => Some("Сбросить все настройки и перезапустить"),
        TextKey::SettingsGraphicsLighting => Some("Освещение"),
        TextKey::SettingsGraphicsEffects => Some("Эффекты"),
        TextKey::SoloViewHint => Some("Показать только эту часть"),
        TextKey::SoloViewRestoreHint => Some("Вернуть предыдущий вид"),
        TextKey::ToneCurveFilmic => Some("Filmic"),
        TextKey::ToneCurveSoft => Some("Мягкая"),
        TextKey::LooksHiddenForOtherFigure => Some("Образы для другой фигуры скрыты"),
        TextKey::LookBelongsToOtherFigure => Some("Этот образ создан для другой фигуры"),
        TextKey::MorphEdit => Some("Правка морфов"),
        TextKey::VaMMorphPairImported => Some("Пара морф-файлов VaM импортирована"),
        TextKey::VaMMorphPairRejected => Some("Пару морф-файлов VaM импортировать нельзя"),
        TextKey::MorphCategoryAll => Some("Все"),
        TextKey::MorphCategoryEyes => Some("Глаза"),
        TextKey::MorphCategoryBrows => Some("Брови"),
        TextKey::MorphCategoryNose => Some("Нос"),
        TextKey::MorphCategoryMouth => Some("Рот"),
        TextKey::MorphCategoryJaw => Some("Челюсть"),
        TextKey::MorphCategoryCheeks => Some("Щёки"),
        TextKey::MorphCategoryEars => Some("Уши"),
        TextKey::MorphCategoryHead => Some("Лицо"),
        TextKey::MorphCategoryExpression => Some("Мимика"),
        TextKey::NoMatchingMorphs => Some("Подходящих морфов нет"),
        TextKey::ResetMorphs => Some("Сбросить все"),
        TextKey::UndoMorphReset => Some("Отменить сброс"),
        TextKey::LightRotationGesture => Some("Shift+Перетаскивание ПКМ"),
        TextKey::UpdateAvailable => Some("Обновить"),
        TextKey::PackageFromFiles => Some("Выбрать файлы"),
        TextKey::PackageMorphFiles => Some("Морфы"),
        TextKey::PackageTextureFiles => Some("Текстуры"),
        TextKey::PackageListEmpty => Some("Пока ничего не добавлено"),
        TextKey::LightingStudio => Some("Студийный"),
        TextKey::LightingSoft => Some("Мягкий"),
        TextKey::LightingDaylight => Some("Дневной"),
        TextKey::LightingWarm => Some("Тёплый"),
        TextKey::LightingGloss => Some("Проверка бликов"),
        TextKey::LightingPortrait => Some("Портрет"),
        TextKey::ImportLoadingMesh => Some("Загрузка меша"),
        TextKey::ImportSimplifying => Some("Упрощение меша"),
        TextKey::StageOptimizeHead => Some("Оптимизация головы"),
        TextKey::StagePrepareInput => Some("Подготовка данных"),
        TextKey::StageAlignScan => Some("Выравнивание скана"),
        TextKey::StageFitShape => Some("Подгонка формы G2"),
        TextKey::StageValidate => Some("Проверка структуры"),
        TextKey::StagePrepareResult => Some("Подготовка результата"),
        TextKey::HeavyMeshNote => Some("около {count} треугольников -- чем больше, тем дольше"),
        TextKey::TransformGroups => Some("Части"),
        TextKey::CollapseTransformGroups => Some("Свернуть части"),
        TextKey::ExpandTransformGroups => Some("Развернуть части"),
        TextKey::Skin => Some("Кожа"),
        TextKey::SculptTear => Some("Слёзная плёнка"),
        TextKey::SculptLips => Some("Губы"),
        TextKey::SculptTeethTongue => Some("Зубы · Язык"),
        TextKey::SculptInnerMouth => Some("Полость рта"),
        TextKey::SculptEyes => Some("Глаза"),
        TextKey::Size => Some("Размер"),
        TextKey::SculptXSymmetry => Some("Симметрия X"),
        TextKey::SculptXSymmetryTooltip => Some("Применяет один и тот же мазок к обеим сторонам"),
        TextKey::BackfaceProtection => Some("Защита задних граней"),
        TextKey::AutoGroup => Some("Автогруппа"),
        TextKey::AutoGroupTooltip => {
            Some("Деформирует только в пределах одной полигональной группы")
        }
        TextKey::EyeTrackingAuto => Some("Авто"),
        TextKey::EyeGazeFreeze => Some("Зафиксировать как морф"),
        TextKey::EyeGazeFreezeTooltip => Some(
            "Закрепляет текущее направление взгляда в морфах, чтобы вкладка сохранения экспортировала эту позу глаз",
        ),
        TextKey::Reset => Some("Сброс"),
        TextKey::SculptGrab => Some("Захват"),
        TextKey::SculptSmooth => Some("Сглаживание"),
        TextKey::SculptPushPull => Some("Вдавить · Вытянуть"),
        TextKey::ShortcutSculptGrab => Some("Тянуть"),
        TextKey::ShortcutSculptSmooth => Some("Shift + тянуть"),
        TextKey::ShortcutSculptPushPull => Some("Ctrl / Alt + тянуть"),
        TextKey::ResetSculpt => Some("Сбросить деформацию"),
        #[cfg(test)]
        TextKey::SculptApplied => Some("Правки скульптинга применены"),
        TextKey::SculptFailed => Some("Операция скульптинга не удалась"),
        TextKey::Ready => Some("Готово"),
        TextKey::Busy => Some("Редактирование заблокировано во время генерации"),
        TextKey::NeedScan => Some(
            "Сначала откройте скан головы в формате OBJ, GLB или FBX либо начните с образа, уже установленного в VaM",
        ),
        TextKey::ScanLoadFailed => Some("Не удалось прочитать этот файл головы"),
        TextKey::NeedEyeMorph => Some("Встроенный морф глаз несовместим с этим G2"),
        TextKey::ReviewEyePins => Some("Проверьте красные точки на веках"),
        TextKey::ResultUnavailable => Some("Сначала подгоните лицо"),
        TextKey::MorphNotInLibrary => {
            Some("Этого морфа ещё нет в библиотеке — каталог загружается.")
        }
        TextKey::SculptNeedsBaseHead => {
            Some("Сначала продолжите с базовой головой на вкладке создания.")
        }
        TextKey::MorphUnavailable => Some("Совместимый итоговый морф недоступен"),
        TextKey::AlignmentPending => Some("Загрузите скан головы и выберите папку VaM"),
        TextKey::ScanLoaded => Some("Пользовательская голова OBJ, GLB или FBX загружена"),
        TextKey::TemplateLoaded => Some("G2 загружен"),
        TextKey::PinsReset => Some("Все точки сброшены"),
        TextKey::ResultStale => Some("После изменений результат нужно создать заново"),
        TextKey::GenerationStarted => Some("Подгонка лица"),
        TextKey::Cancelling => Some("Отмена подгонки лица"),
        TextKey::GenerationComplete => Some("Подгонка лица завершена"),
        TextKey::GenerationCancelled => Some("Подгонка лица отменена"),
        TextKey::GenerationFailed => Some("Подгонка лица не удалась"),
        TextKey::ExportStarted => Some("Экспорт"),
        TextKey::ExportComplete => Some("Экспорт завершён"),
        TextKey::ExportFailed => Some("Экспорт не удался"),
        TextKey::MorphNameHint => Some("Введите имя морфа"),
        TextKey::MorphGroupHint => Some("Группа  (напр. Custom, Morphs/Morph Loader)"),
        TextKey::MorphRegionHint => Some("Раздел  (напр. Custom, Morphs/Body)"),
        TextKey::DirectEntry => Some("Ввести вручную"),
        TextKey::VaMMetadataPoseMorph => Some("Зарегистрировать как морф позы"),
        TextKey::VaMMetadataPoseMorphTooltip => Some("Обрабатывается вместе с мимикой и позами"),
        TextKey::VaMMetadataBoneCorrection => Some("Коррекция скелета"),
        TextKey::VaMMetadataBoneCorrectionTooltip => {
            Some("Заново центрирует опорные точки на изменённом скелете")
        }
        TextKey::NativeCorePending => Some("Идёт интеграция нативного ядра подгонки"),
        TextKey::MorphPreviewUpdated => Some("Предпросмотр морфа обновлён"),
        TextKey::MorphsReset => Some("Морфы сброшены"),
        TextKey::MorphsResetUndone => Some("Сброс морфов отменён"),
        TextKey::MorphEditUndone => Some("Правка морфа отменена"),
        TextKey::MorphApplied => Some("Морф применён"),
        TextKey::HelpPlace => Some("Поставить точку"),
        TextKey::HelpMove => Some("Переместить точку"),
        TextKey::HelpDelete => Some("Удалить точку"),
        TextKey::HelpXSymmetry => Some("Симметрия точек по X"),
        TextKey::HelpSnapRotation => Some("Привязка поворота"),
        TextKey::HelpDragZoom => Some("Зум перетаскиванием"),
        TextKey::HelpOrbit => Some("Орбита"),
        TextKey::HelpPan => Some("Панорамирование"),
        TextKey::HelpZoom => Some("Зум"),
        TextKey::HelpLight => Some("Вращать свет"),
        TextKey::HelpFrameView => Some("Вписать голову"),
        TextKey::HelpUndo => Some("Отменить"),
        TextKey::ShortcutPlace => Some("ЛКМ"),
        TextKey::ShortcutMove => Some("Alt + тянуть"),
        TextKey::ShortcutDelete => Some("ПКМ"),
        TextKey::ShortcutXSymmetry => Some("X"),
        TextKey::ShortcutSnapRotation => Some("Shift / Ctrl при повороте"),
        TextKey::ShortcutDragZoom => Some("Ctrl + тянуть СКМ вертикально"),
        TextKey::ShortcutOrbit => Some("Тянуть СКМ"),
        TextKey::ShortcutPan => Some("Shift + тянуть СКМ"),
        TextKey::ShortcutZoom => Some("Колесо мыши"),
        TextKey::ShortcutLight => Some("Shift + тянуть ПКМ"),
        TextKey::ShortcutFrameView => Some("F"),
        TextKey::HelpSnapView => Some("Привязка вида"),
        TextKey::ShortcutSnapView => Some("Alt + тянуть колесом"),
        TextKey::HelpStandardViews => Some("Стандартные виды"),
        TextKey::ShortcutStandardViews => Some("Numpad 1-9"),
        TextKey::ShortcutUndo => Some("Ctrl+Z"),
        TextKey::TemplatePending => {
            Some("Выберите папку VaM, чтобы база G2 подготовилась автоматически")
        }
        TextKey::ResultEmpty => Some("Готового морфа нет"),
        TextKey::ScaleLink => Some("Связать масштаб"),
        TextKey::ScaleLinkTooltip => Some("Связывает значения масштаба X/Y/Z"),
        TextKey::CameraSettings => Some("Настройки камеры"),
        TextKey::Perspective => Some("Перспективная"),
        TextKey::Orthographic => Some("Ортогональная"),
        TextKey::Fov => Some("FOV"),
        TextKey::ResetCamera => Some("Сбросить камеру"),
        TextKey::WindowMinimize => Some("Свернуть"),
        TextKey::WindowMaximize => Some("Развернуть"),
        TextKey::WindowRestore => Some("Восстановить"),
        TextKey::WindowClose => Some("Закрыть"),
        TextKey::TooltipShow => Some("Показать"),
        TextKey::TooltipHide => Some("Скрыть"),
        TextKey::TooltipLock => Some("Закрепить"),
        TextKey::TooltipUnlock => Some("Открепить"),
        TextKey::AdjustEyeGaze => Some("Настроить направление взгляда"),
        TextKey::ScanLoading => Some("Загрузка скана головы"),
        TextKey::TemplateLoading => Some("Загрузка шаблона G2"),
        TextKey::ResultLoading => Some("Загрузка итогового меша"),
        TextKey::VaMMorphPairLoading => Some("Загрузка пары морф-файлов VaM"),
        TextKey::SystemLog => Some("Системный журнал"),
        TextKey::Strength => Some("Сила"),
        TextKey::StrengthShort => Some("Сила"),
        TextKey::SculptBrushMove => Some("Захват"),
        TextKey::SculptBrushSmooth => Some("Сглаживание"),
        TextKey::SculptBrushRestore => Some("Восстановить"),
        TextKey::SculptBrushMoveTooltip => {
            Some("Тянет поверхность за курсором; сила задаёт, насколько далеко она следует")
        }
        TextKey::SculptBrushSmoothTooltip => Some("Сглаживает детали поверхности"),
        TextKey::SculptBrushRestoreTooltip => Some("Возвращает поверхность к базовой форме"),
        TextKey::TextureLayers => Some("Слои текстур"),
        TextKey::PackageSection => Some("Экспорт VAR"),
        TextKey::PackageCreatorHint => Some("Автор"),
        TextKey::PackageNameHint => Some("Имя пакета"),
        TextKey::PackageDescriptionHint => Some("Описание"),
        TextKey::PackageCreditsHint => Some("Благодарности"),
        TextKey::PackageInstructionsHint => Some("Инструкция"),
        TextKey::PackageLinkHint => Some("Ссылка на поддержку"),
        TextKey::PackageSave => Some("Сохранить VAR"),
        TextKey::PackageFromThisHead => Some("Создать из этой головы"),
        TextKey::PackageAddMorphs => Some("Добавить морфы"),
        TextKey::PackageAddTextures => Some("Добавить текстуры"),
        TextKey::PackageNeedsVamRestart => Some("Требуется перезапуск VaM"),
        TextKey::PackageSavedTo => Some("Сохранено в"),
        TextKey::FieldRequired => Some("(обязательно)"),
        TextKey::FieldOptional => Some("(необязательно)"),
        TextKey::PackageSaveTo => Some("Сохранить в"),
        TextKey::PackageNothingToPack => {
            Some("Нечего упаковывать. Включите эту голову или добавьте файл.")
        }
        TextKey::PackageVersionMarker => Some("V"),
        TextKey::PackageCreatorRequired => Some("Нужно указать имя автора."),
        TextKey::PackageNameRequired => Some("Нужно указать имя пакета."),
        TextKey::PackageReplaceTitle => Some("VAR с таким именем уже есть"),
        TextKey::PackageReplaceBody => {
            Some("Заменить? Сцены, ссылающиеся на эту версию, будут читать новое содержимое.")
        }
        TextKey::LicenseByTooltip => Some("указание автора"),
        TextKey::LicenseBySaTooltip => Some("указание автора / на тех же условиях"),
        TextKey::LicenseByNdTooltip => Some("указание автора / без производных"),
        TextKey::LicenseByNcTooltip => Some("указание автора / некоммерческое использование"),
        TextKey::LicenseByNcSaTooltip => {
            Some("указание автора / некоммерческое использование / на тех же условиях")
        }
        TextKey::LicenseByNcNdTooltip => {
            Some("указание автора / некоммерческое использование / без производных")
        }
        TextKey::LicensePcEaTooltip => Some("ранний доступ для спонсоров, без распространения"),
        TextKey::AddTextureLayer => Some("Добавить изображение"),
        TextKey::AddG2UvTextureLayer => Some("Добавить UV-карту"),
        TextKey::AddTextureLayerTooltip => {
            Some("Размещает фотографию лица на голове и правит её как текстуру")
        }
        TextKey::AddG2UvTextureLayerTooltip => {
            Some("Добавляет текстуру face для G2 в её собственной UV-развёртке и правит её")
        }
        TextKey::AddTextureLayerHint => {
            Some("Добавьте изображение лица или текстуру G2 кнопкой +.")
        }
        TextKey::BakingTextures => Some("Запекание текстур…"),
        TextKey::BakeTexturesForSave => Some("Запечь текстуры для сохранения"),
        TextKey::TextureMetalRough => Some("Metallic / Roughness"),
        TextKey::TextureGlossSmooth => Some("Glossiness / Smoothness"),
        TextKey::LayerOpacity => Some("Непрозрачность слоя"),
        TextKey::PinOpacity => Some("Непрозрачность точек"),
        TextKey::TransferPins => Some("Перенести точки"),
        TextKey::TransferPinsTooltip => {
            Some("Копирует расставленные точки на изображения других слоёв.")
        }
        TextKey::DeleteLayer => Some("Удалить слой"),
        TextKey::TextureNormalStrength => Some("Сила нормалей"),
        TextKey::TextureInvertScalar => Some("Инвертировать скалярные значения"),
        TextKey::BlendNormal => Some("Обычный"),
        TextKey::BlendMultiply => Some("Умножение"),
        TextKey::BlendScreen => Some("Осветление"),
        TextKey::BlendOverlay => Some("Перекрытие"),
        TextKey::MatchToneToLayerBelow => Some("Подогнать тон к нижнему слою"),
        TextKey::ImageMirror => Some("Зеркало влево/вправо"),
        TextKey::ImageMirrorOff => Some("Выкл"),
        TextKey::ImageMirrorToLeft => Some("Влево"),
        TextKey::ImageMirrorToRight => Some("Вправо"),
        TextKey::ImageAdjustments => Some("Коррекция изображения"),
        TextKey::Exposure => Some("Экспозиция"),
        TextKey::Contrast => Some("Контраст"),
        TextKey::Saturation => Some("Насыщенность"),
        TextKey::Hue => Some("Оттенок"),
        TextKey::Temperature => Some("Температура"),
        TextKey::TextureMaskPreviewTooltip => {
            Some("Показывает маску активного слоя полупрозрачным красным наложением")
        }
        TextKey::ClearLayerMask => Some("Очистить маску слоя"),
        TextKey::ResetSourceRetouch => Some("Сбросить ретушь источника"),
        TextKey::CloneSampleHint => {
            Some("Alt-щелчок по исходному изображению выбирает образец для клонирования.")
        }
        TextKey::BakeBase => Some("Параметры запекания"),
        TextKey::TransparentOverlay => Some("Запечь только слои"),
        TextKey::CurrentSkinBake => Some("Запечь с кожей VaM"),
        TextKey::TransparentOverlayTooltip => {
            Some("Запекает только изображения слоёв, без пресета кожи")
        }
        TextKey::CurrentSkinBakeTooltip => Some("Запекает слои поверх пресета кожи как основы"),
        TextKey::SkinPresetRequired => Some("Сначала выберите пресет в разделе «Настройки кожи»"),
        TextKey::SkinSettings => Some("Настройки кожи"),
        TextKey::ScanHeadColor => Some("Цвет скана головы"),
        TextKey::G2HeadColor => Some("Цвет головы G2"),
        TextKey::MorphFilterAll => Some("Все"),
        TextKey::MorphFilterActive => Some("Активные"),
        TextKey::BrushDodge => Some("Осветлитель"),
        TextKey::BrushBurn => Some("Затемнитель"),
        TextKey::BrushSaturate => Some("Насытить"),
        TextKey::BrushDesaturate => Some("Обесцветить"),
        TextKey::ShowLayersOnly => Some("Только слои"),
        TextKey::BaseWithoutSkin => Some("Слой"),
        TextKey::BaseWithoutSkinTooltip => Some(
            "Нарисованные слои на сплошном цвете, без пресета; экспортируемая текстура не меняется",
        ),
        TextKey::SolidColorTooltip => Some("Только сплошной цвет, без текстуры"),
        TextKey::TextureSkinTooltip => Some("Нарисованные слои поверх пресета кожи VaM"),
        TextKey::BoundaryFeather => Some("Растушевать край лица"),
        TextKey::BoundaryFeatherTooltip => Some(
            "Только диффуз. Карты вроде нормалей и шероховатости нельзя смешивать через альфу, поэтому их край остаётся жёстким",
        ),
        TextKey::Baking => Some("Запекание…"),
        TextKey::TextureToolPinPair => {
            Some("Парные точки — ставьте соответствующие точки на 3D-лице и исходном изображении")
        }
        TextKey::TextureToolMask => Some("Маска — рисуйте, чтобы открыть; Alt — чтобы скрыть"),
        TextKey::TextureToolClone => {
            Some("Штамп — Alt-щелчок задаёт образец, затем рисуйте, чтобы копировать")
        }
        TextKey::TextureToolDodgeBurn => {
            Some("Осветлитель/Затемнитель — осветляет; Alt — затемняет")
        }
        TextKey::TextureToolSponge => Some("Губка — добавляет насыщенность; Alt — убирает её"),
        TextKey::SelectTextureLayer => Some("Выберите слой текстуры."),
        TextKey::LoadingTextureImage => Some("Загрузка изображения…"),
        TextKey::ScanTextureAlignment => {
            Some("Текстура скана наследует выполненное 3D-выравнивание.")
        }
        TextKey::NoTextureImage => Some("Нет изображения."),
        TextKey::MorphSavedReloadFemale => {
            Some("Нажмите Reload Custom Morphs на вкладке Female Morphs в VaM, чтобы обновить.")
        }
        TextKey::MorphSavedReloadMale => {
            Some("Нажмите Reload Custom Morphs на вкладке Male Morphs в VaM, чтобы обновить.")
        }
        TextKey::BootStarting => Some("Подготовка Vkit"),
        TextKey::BootFonts => Some("Загрузка шрифтов"),
        TextKey::BootGraphics => Some("Запуск графики"),
        TextKey::BootSettings => Some("Загрузка настроек"),
        TextKey::BootTemplate => Some("Загрузка G2"),
        TextKey::BootWorkspace => Some("Подготовка рабочей области"),
        TextKey::BootReady => Some("Открытие"),
        TextKey::StartupFailedTitle => Some("Не удалось запустить Vkit"),
        TextKey::StartupFailedGraphics => Some(
            "Vkit рисует средствами DirectX 12, а графика этого компьютера не смогла их предоставить. Обновите драйвер видеокарты или запустите Vkit на компьютере с GPU, поддерживающим DirectX 12. У сеанса удалённого рабочего стола или виртуальной машины часто нет собственного дисплея DirectX 12.",
        ),
        TextKey::StartupFailedOther => {
            Some("Vkit остановился во время запуска и не смог открыть своё окно.")
        }
        TextKey::StartupFailedLogHint => Some("Что именно пошло не так, записано в:"),
        TextKey::DialogOpenScan => Some("Открыть OBJ, GLB или FBX головы"),
        TextKey::DialogAddUvLayer => Some("Добавить слой текстуры G2 UV"),
        TextKey::DialogAddTextureLayer => Some("Добавить слой текстуры"),
        TextKey::DialogSaveMorphPair => Some("Сохранить морф VaM VMI + VMB"),
        TextKey::DialogChooseVamFolder => Some("Выбрать папку Virt-A-Mate"),
        _ => None,
    }
}
