use super::TextKey;

pub(super) const fn label(key: TextKey) -> Option<&'static str> {
    #[allow(unreachable_patterns)]
    match key {
        TextKey::SurfaceSmooth => Some("Penghalusan mesh"),
        TextKey::SurfaceSmoothTooltip => Some(
            "Penghalusan render yang kompatibel dengan VaM. 0 adalah mesh dasar yang bisa disunting, 3 adalah bawaan VaM, dan 4 adalah maksimum. Morph, UV, sculpting, dan bake tekstur tetap mempertahankan topologi dasar",
        ),
        TextKey::FaceMatch => Some("Buat"),
        TextKey::ResultPreview => Some("Simpan"),
        TextKey::Save => Some("Simpan"),
        TextKey::DetailCorrection => Some("Sculpt"),
        TextKey::TextureStage => Some("Tekstur"),
        TextKey::MorphCategoryBody => Some("Badan"),
        TextKey::TextureNamePlaceholder => Some("Masukkan nama tekstur"),
        TextKey::TextureOverwriteTitle => Some("Ini akan mengganti tekstur bernama sama"),
        TextKey::OpenScan => Some("Pindai Kepala"),
        TextKey::SourceMorphMissing => Some("Tidak ada morph look ini yang terpasang"),
        TextKey::EditDetails => Some("Sunting Detail"),
        TextKey::Female => Some("Perempuan"),
        TextKey::Male => Some("Laki-laki"),
        TextKey::Lighting => Some("Pencahayaan"),
        TextKey::Brightness => Some("Kecerahan"),
        TextKey::LightRotation => Some("Rotasi Cahaya"),
        TextKey::AddHeadFileHint => Some("atau seret berkas OBJ, GLB, atau FBX ke sini"),
        TextKey::AddHeadFile => Some("Tambah berkas kepala"),
        TextKey::AutoAlign => Some("Cocokkan Otomatis"),
        TextKey::PlacementReset => Some("Atur Ulang Posisi"),
        TextKey::Scale => Some("Skala"),
        TextKey::Position => Some("Posisi"),
        TextKey::Rotation => Some("Rotasi"),
        TextKey::XMirror => Some("Cermin X"),
        TextKey::XMirrorTooltip => Some("Memasang satu pin juga memasang kembarannya di sisi lain"),
        TextKey::NumbersTooltip => Some("Beri nomor setiap pin sesuai urutan pemasangan"),
        TextKey::XMirrorRejected => Some("Cermin X dimatikan karena tidak ada pasangan yang aman"),
        TextKey::ScanFidelity => Some("Ketelitian pindai"),
        TextKey::ScanFidelityTooltip => Some(
            "Di atas 1, hasil fit lebih mengikuti bentuk pindaian dan lebih sedikit mempertahankan kehalusan figur. Sampai 2 hanya itu yang terjadi; lewat 2, derau pindaian -- bintik, lubang, garis jahitan -- ikut muncul sebagai tonjolan, dan pindaian yang bersih serta rapat bisa layak didorong ke sana. Pin tidak pernah dikorbankan pada setelan mana pun, dan tidak ada pengaman yang dilonggarkan.",
        ),
        TextKey::ScanSymmetry => Some("Simetri Kepala Pindaian"),
        TextKey::Original => Some("Asli"),
        TextKey::Left => Some("Kiri"),
        TextKey::Right => Some("Kanan"),
        TextKey::EyesOpen => Some("Mata Terbuka"),
        TextKey::EyesClosed => Some("Mata Tertutup"),
        TextKey::View => Some("Opsi Tampilan"),
        TextKey::On => Some("Aktif"),
        TextKey::Numbers => Some("Nomor"),
        TextKey::PinPairsPlaced => Some("Pin"),
        TextKey::PinPlacement => Some("Pemasangan pin"),
        TextKey::ScanReference => Some("Acuan pindaian"),
        TextKey::AddHeadFileAction => Some("Tambah berkas kepala"),
        TextKey::AddHeadFileActionTooltip => {
            Some("Muat berkas kepala hasil pindai (OBJ · GLB · FBX)")
        }
        TextKey::ContinueStep => Some("Lanjut dengan kepala dasar"),
        TextKey::ContinueStepTooltip => Some(
            "Sunting kepala G2 apa adanya, tanpa pindaian; pindaian tetap bisa dimuat dan difit nanti",
        ),
        TextKey::Undo => Some("Urungkan"),
        TextKey::ResetAllPins => Some("Atur Ulang Semua Pin"),
        TextKey::MorphSave => Some("Simpan morph"),
        TextKey::TextureSaveSection => Some("Simpan tekstur"),
        TextKey::Generate => Some("Fit Wajah"),
        TextKey::Cancel => Some("Batal"),
        TextKey::CopyAll => Some("Salin Semua"),
        TextKey::Next => Some("Simpan"),
        TextKey::TextureSkin => Some("Preset"),
        TextKey::SolidColor => Some("Solid"),
        TextKey::BackfaceProtectionTooltip => Some("Melindungi sisi belakang mesh tipis"),
        TextKey::WireframeColor => Some("Warna Wireframe"),
        TextKey::WireframeSettings => Some("Wireframe"),
        TextKey::XraySettings => Some("X-ray"),
        TextKey::ViewportLightingTooltip => Some("Atur pencahayaan"),
        TextKey::Background => Some("Latar"),
        TextKey::ViewportBackgroundTooltip => Some("Atur latar dan gambar acuan"),
        TextKey::BackgroundRadial => Some("Radial"),
        TextKey::BackgroundVertical => Some("Vertikal"),
        TextKey::BackgroundFlat => Some("Polos"),
        TextKey::ViewportWireframeTooltip => Some("Tampilkan dan atur hamparan Wireframe"),
        TextKey::ViewportXrayTooltip => Some("Tampilkan dan atur hamparan X-ray"),
        TextKey::ViewportSkinTooltip => Some("Pilih tekstur kulit VaM"),
        TextKey::ViewportHairTooltip => Some("Pilih preset rambut VaM"),
        TextKey::FalloffTooltip => Some("Pilih kurva pengaruh kuas"),
        TextKey::FalloffSmooth => Some("Halus"),
        TextKey::FalloffSmoother => Some("Lebih Halus"),
        TextKey::FalloffSharp => Some("Tajam"),
        TextKey::FalloffLinear => Some("Linear"),
        TextKey::PlaceHead => Some("Tempatkan Kepala"),
        TextKey::PlaceHeadTooltip => Some(
            "Menumpuk kepala yang dimuat di atas G2 untuk menyelaraskan posisi, rotasi, dan ukurannya.",
        ),
        TextKey::ScanOverlay => Some("Hamparan Pindaian"),
        TextKey::OverlayDone => Some("Selesaikan Hamparan"),
        TextKey::ProjectImage => Some("Proyeksi"),
        TextKey::ProjectionCanvasHint => {
            Some("Lukis di tampilan 3D dan hasilnya jatuh ke kanvas UV ini")
        }
        TextKey::HelpProjectionPaint => Some("Lukis (proyeksi)"),
        TextKey::ShortcutProjectionPaint => Some("Seret kiri"),
        TextKey::HelpProjectionMove => Some("Geser gambar"),
        TextKey::ShortcutProjectionMove => Some("Seret kanan"),
        TextKey::HelpProjectionRotate => Some("Putar gambar"),
        TextKey::ShortcutProjectionRotate => Some("Shift+seret kanan"),
        TextKey::HelpProjectionZoom => Some("Skala gambar"),
        TextKey::ShortcutProjectionZoom => Some("Roda"),
        TextKey::HelpBrushSize => Some("Ukuran kuas"),
        TextKey::ShortcutBrushSize => Some("Seret F · [ ]"),
        TextKey::HelpBrushStrength => Some("Kekuatan kuas"),
        TextKey::ShortcutBrushStrength => Some("Shift+F seret"),
        TextKey::HelpProjectionDone => Some("Selesaikan proyeksi"),
        TextKey::ShortcutProjectionDone => Some("Selesai · Esc"),
        TextKey::ProjectDone => Some("Selesai"),
        TextKey::ProjectDoneTooltip => {
            Some("Selesaikan proyeksi; semua yang dilukis tetap ada di lapisan")
        }
        TextKey::MorphCompare => Some("Bandingkan"),
        TextKey::Opacity => Some("Opasitas"),
        TextKey::G2Head => Some("Kepala G2"),
        TextKey::Eyelashes => Some("Bulu Mata"),
        TextKey::OverwriteTitle => Some("Timpa Berkas"),
        TextKey::OverwriteBody => Some("Berkas dengan nama yang sama sudah ada. Timpa?"),
        TextKey::Overwrite => Some("Timpa"),
        TextKey::ScanTextureLayer => Some("Tekstur pindaian"),
        TextKey::PinPrompt => Some("Tempatkan pin pada masing-masing, atau lanjut tanpa pin"),
        TextKey::TextureNeedsImage => Some("Tambahkan gambar dari panel samping"),
        TextKey::TexturePinPairPrompt => Some("Tempatkan pin berpasangan di kiri dan kanan"),
        TextKey::SymmetrySuggestion => Some("Pencerminan disarankan agar hasilnya lebih rapi"),
        TextKey::SymmetryChangeTitle => Some("Pin akan diatur ulang"),
        TextKey::SymmetryChangeConfirm => Some("Konfirmasi"),
        TextKey::EyeClosure => Some("Tutup Mata"),
        TextKey::VaMFolder => Some("Folder VaM"),
        #[cfg(test)]
        TextKey::VaMGeometryBaseReady => Some("Basis geometri VaM terverifikasi"),
        TextKey::VaMGeometryBaseRejected => Some("Basis geometri VaM tidak dapat dipakai"),
        TextKey::VaMGeometryBaseHowTo => {
            Some("Pilih folder VaM Anda; basis G2 netral diekstrak otomatis dari instalasi Anda")
        }
        TextKey::VaMBaseSourceUnityBundle => Some("Basis netral bawaan VaM (ekstrak otomatis)"),
        TextKey::VaMBaseSourceDazAnchorPair => Some("Jangkar DAZ + basis OBJ terverifikasi"),
        TextKey::VaMBaseSourceUnityAnchorPair => {
            Some("Jangkar netral hasil ekstrak + basis OBJ terverifikasi")
        }
        TextKey::VaMBaseSourceUnverifiedObj => Some("Basis OBJ belum terverifikasi"),
        TextKey::VaMUvMappingUnavailable => Some(
            "Pemetaan UV kulit VaM belum siap. Periksa folder VaM Anda dan segarkan katalog kulit",
        ),
        TextKey::VaMIndexing => Some("Mengindeks katalog VaM"),
        TextKey::VaMFailed => Some("Katalog VaM gagal"),
        TextKey::RefreshSkins => Some("Segarkan pustaka kulit"),
        TextKey::MorphLoading => Some("Memuat morph"),
        TextKey::MorphLoadFailed => Some("Gagal memuat morph"),
        TextKey::DefaultSkinMissing => Some("Kulit itu tidak ditemukan. Periksa folder VaM"),
        TextKey::DefaultSkinSet => Some("Buka dengan kulit ini"),
        TextKey::DefaultSkinClear => Some("Berhenti membuka dengan kulit ini"),
        TextKey::SkinNone => Some("Tidak ada"),
        TextKey::SkinSearch => Some("Cari Kulit"),
        TextKey::SkinLoading => Some("Memuat kulit"),
        TextKey::SkinReady => Some("Pratinjau kulit siap"),
        TextKey::SkinLoadFailed => Some("Gagal memuat kulit"),
        TextKey::SkinUvUnavailable => Some("UV kulit tidak tersedia"),
        TextKey::NoMatchingSkins => Some("Tidak ada kulit yang cocok"),
        TextKey::Hair => Some("Rambut"),
        TextKey::HairNone => Some("Tidak ada"),
        TextKey::HairSearch => Some("Cari Rambut"),
        TextKey::HairLoading => Some("Memuat rambut"),
        TextKey::HairReady => Some("Pratinjau rambut siap"),
        TextKey::HairLoadFailed => Some("Gagal memuat rambut"),
        TextKey::HairPartsSkipped => Some("Ditampilkan tanpa beberapa bagian"),
        TextKey::HairParts => Some("bagian"),
        TextKey::BaseFace => Some("Wajah dasar"),
        TextKey::Settings => Some("Pengaturan"),
        TextKey::SettingsGraphics => Some("Grafis"),
        TextKey::SettingsViewport => Some("Viewport"),
        TextKey::SettingsGeneral => Some("Umum"),
        TextKey::SettingsAbout => Some("Tentang"),
        TextKey::SettingsAboutBuild => Some("Build"),
        TextKey::SettingsAboutLicense => Some("Lisensi"),
        TextKey::SettingsAboutDiagnostics => Some("Diagnostik"),
        TextKey::SettingsTooltipsTooltip => Some("Bantuan seperti ini, yang barusan muncul"),
        TextKey::AboutTagline => Some("Alat pendamping untuk membuat morph kepala VaM."),
        TextKey::AboutNoAffiliation => Some(
            "Virt-A-Mate milik MeshedVR dan Genesis 2 milik DAZ 3D. Vkit adalah alat independen yang tidak berafiliasi dengan keduanya.",
        ),
        TextKey::AboutNoWarranty => Some(
            "Disediakan apa adanya, tanpa jaminan. Apa yang Anda buat dan lisensi konten yang Anda muat adalah tanggung jawab Anda.",
        ),
        TextKey::AboutSupport => Some("Dukungan"),
        TextKey::AboutReportProblem => Some(
            "Vkit menulis log setiap kali dijalankan, dan berkas tersendiri jika ia tumbang. Buka foldernya dan lampirkan keduanya pada laporan: kesalahan yang tak terlihat siapa pun adalah kesalahan yang tak bisa diperbaiki siapa pun.",
        ),
        TextKey::AboutOpenLogs => Some("Buka folder log"),
        TextKey::AboutReportIssue => Some("Laporkan masalah"),
        TextKey::SettingsLightingGroup => Some("Pencahayaan"),
        TextKey::SettingsLightingPreset => Some("Preset"),
        TextKey::SettingsExposure => Some("Eksposur"),
        TextKey::SettingsGeometryGroup => Some("Geometri"),
        TextKey::SettingsSmoothPasses => Some("Penghalusan permukaan"),
        TextKey::SettingsSmoothPassesHint => Some(
            "Hanya menghaluskan tampilan, seperti yang dilakukan VaM. Bentuk yang disimpan tidak berubah.",
        ),
        TextKey::SettingsBackgroundGroup => Some("Latar"),
        TextKey::SettingsBackground => Some("Gaya"),
        TextKey::SettingsOverlayGroup => Some("Hamparan"),
        TextKey::SettingsWireframeColor => Some("Warna wireframe"),
        TextKey::SettingsWireframeOpacity => Some("Opasitas wireframe"),
        TextKey::SettingsXrayOpacity => Some("Opasitas X-ray"),
        TextKey::SettingsSolidColor => Some("Warna solid kepala"),
        TextKey::SettingsLanguageGroup => Some("Bahasa"),
        TextKey::SettingsMorphNames => Some("Nama morph bawaan"),
        TextKey::SettingsMorphNamesTranslated => Some("Diterjemahkan"),
        TextKey::SettingsMorphNamesOriginal => Some("Asli bahasa Inggris"),
        TextKey::SettingsLanguage => Some("Bahasa tampilan"),
        TextKey::SettingsFoldersGroup => Some("Folder"),
        TextKey::SettingsVaMRoot => Some("Jalur instalasi VaM"),
        TextKey::HairVisible => Some("Tampilkan rambut"),
        TextKey::HairNeedsFinishedHead => Some("Rambut dipakai pada kepala yang sudah jadi."),
        TextKey::HairPreviewCaveat => Some("Detail rambut berbeda dari aslinya."),
        TextKey::NoMatchingHair => Some("Tidak ada preset rambut yang cocok"),
        TextKey::MorphSearch => Some("Cari Morph"),
        TextKey::MorphOneSidedFilter => Some("K/Ka"),
        TextKey::MorphOneSidedFilterShow => {
            Some("Tampilkan morph yang menggerakkan satu sisi saja")
        }
        TextKey::MorphOneSidedFilterHide => {
            Some("Sembunyikan morph yang menggerakkan satu sisi saja")
        }
        TextKey::AppearanceSearch => Some("Cari Look"),
        TextKey::LookFind => Some("Muat Look"),
        TextKey::CameraTrackballArmed => {
            Some("Rotasi trackball · miringkan dari tepi · klik atau R untuk selesai")
        }
        TextKey::HelpTrackball => Some("Rotasi trackball"),
        TextKey::ShortcutTrackball => Some("R"),
        TextKey::RecoveryTitle => Some("Sesi terakhir berakhir tak terduga"),
        TextKey::RecoveryBody => Some(
            "Morph, sculpting, dan lapisan tekstur Anda tersimpan tepat sebelum berhenti. Kembalikan?",
        ),
        TextKey::RecoveryRestore => Some("Pulihkan"),
        TextKey::RecoveryDiscard => Some("Mulai baru"),
        TextKey::RecoveryRestored => Some("Sesi sebelumnya dipulihkan."),
        TextKey::RecoveryLookMissing => {
            Some("Look dari sesi itu sudah tidak terpasang, jadi tidak ada yang bisa dipulihkan.")
        }
        TextKey::SettingsOcclusionGroup => Some("Ambient occlusion"),
        TextKey::SettingsOcclusionIntensity => Some("Kekuatan"),
        TextKey::SettingsOcclusionRadius => Some("Jangkauan"),
        TextKey::SettingsBloomGroup => Some("Bloom"),
        TextKey::SettingsBloomIntensity => Some("Intensitas"),
        TextKey::SettingsBloomThreshold => Some("Ambang"),
        TextKey::SettingsBloomSoftKnee => Some("Soft knee"),
        TextKey::SettingsBloomRadius => Some("Radius"),
        TextKey::SettingsVignetteGroup => Some("Vignette"),
        TextKey::SettingsVignetteIntensity => Some("Kekuatan"),
        TextKey::SettingsVignetteSmoothness => Some("Kelembutan"),
        TextKey::SettingsVignetteRoundness => Some("Kebulatan"),
        TextKey::SculptNotCarried => Some("Sculpting tidak bisa mengikuti look ini"),
        TextKey::SettingsToneCurve => Some("Kurva tone"),
        TextKey::SettingsToneCurveTooltip => Some("Menyesuaikan bayangan dan sorotan"),
        TextKey::SettingsOcclusionTooltip => Some("Menggelapkan lipatan tempat permukaan bertemu"),
        TextKey::SettingsVignetteTooltip => Some("Menggelapkan tepi bingkai"),
        TextKey::SettingsEffectEnabled => Some("Aktif"),
        TextKey::SettingsInterfaceGroup => Some("Antarmuka"),
        TextKey::SettingsTooltips => Some("Tampilkan tooltip"),
        TextKey::SettingsResetGroup => Some("Atur Ulang"),
        TextKey::SettingsClearCache => Some("Kosongkan cache"),
        TextKey::SettingsClearCacheTooltip => Some(
            "Base hasil ekstraksi dan bank morph, dibangun ulang saat dibutuhkan. Mengosongkannya hanya membuat satu kali start lebih lambat.",
        ),
        TextKey::SettingsResetAll => Some("Setel ulang semua pengaturan lalu mulai ulang"),
        TextKey::SettingsGraphicsLighting => Some("Pencahayaan"),
        TextKey::SettingsGraphicsEffects => Some("Efek"),
        TextKey::SoloViewHint => Some("Tampilkan bagian ini saja"),
        TextKey::SoloViewRestoreHint => Some("Kembalikan tampilan sebelumnya"),
        TextKey::ToneCurveFilmic => Some("Filmic"),
        TextKey::ToneCurveSoft => Some("Lembut"),
        TextKey::LooksHiddenForOtherFigure => Some("Look untuk figur lain disembunyikan"),
        TextKey::LookBelongsToOtherFigure => Some("Look ini dibuat untuk figur lain"),
        TextKey::MorphEdit => Some("Sunting Morph"),
        TextKey::VaMMorphPairImported => Some("Pasangan morph VaM diimpor"),
        TextKey::VaMMorphPairRejected => Some("Pasangan morph VaM tidak dapat diimpor"),
        TextKey::MorphCategoryAll => Some("Semua"),
        TextKey::MorphCategoryEyes => Some("Mata"),
        TextKey::MorphCategoryBrows => Some("Alis"),
        TextKey::MorphCategoryNose => Some("Hidung"),
        TextKey::MorphCategoryMouth => Some("Mulut"),
        TextKey::MorphCategoryJaw => Some("Rahang"),
        TextKey::MorphCategoryCheeks => Some("Pipi"),
        TextKey::MorphCategoryEars => Some("Telinga"),
        TextKey::MorphCategoryHead => Some("Wajah"),
        TextKey::MorphCategoryExpression => Some("Ekspresi"),
        TextKey::NoMatchingMorphs => Some("Tidak ada morph yang cocok"),
        TextKey::ResetMorphs => Some("Atur Ulang Semua"),
        TextKey::UndoMorphReset => Some("Batalkan reset"),
        TextKey::LightRotationGesture => Some("Shift+Seret klik kanan"),
        TextKey::UpdateAvailable => Some("Perbarui"),
        TextKey::PackageFromFiles => Some("Pilih berkas"),
        TextKey::PackageMorphFiles => Some("Morph"),
        TextKey::PackageTextureFiles => Some("Tekstur"),
        TextKey::PackageListEmpty => Some("Belum ada yang ditambahkan"),
        TextKey::LightingStudio => Some("Studio"),
        TextKey::LightingSoft => Some("Lembut"),
        TextKey::LightingDaylight => Some("Cahaya siang"),
        TextKey::LightingWarm => Some("Hangat"),
        TextKey::LightingGloss => Some("Cek kilap"),
        TextKey::LightingPortrait => Some("Potret"),
        TextKey::ImportLoadingMesh => Some("Memuat mesh"),
        TextKey::ImportSimplifying => Some("Menyederhanakan mesh"),
        TextKey::StageOptimizeHead => Some("Optimalkan kepala"),
        TextKey::StagePrepareInput => Some("Menyiapkan masukan"),
        TextKey::StageAlignScan => Some("Menyelaraskan pindaian"),
        TextKey::StageFitShape => Some("Menyesuaikan bentuk G2"),
        TextKey::StageValidate => Some("Memvalidasi struktur"),
        TextKey::StagePrepareResult => Some("Menyiapkan hasil"),
        TextKey::HeavyMeshNote => Some("sekitar {count} segitiga -- makin banyak makin lama"),
        TextKey::TransformGroups => Some("Bagian"),
        TextKey::CollapseTransformGroups => Some("Tutup bagian"),
        TextKey::ExpandTransformGroups => Some("Buka bagian"),
        TextKey::Skin => Some("Kulit"),
        TextKey::SculptTear => Some("Air Mata"),
        TextKey::SculptLips => Some("Bibir"),
        TextKey::SculptTeethTongue => Some("Gigi · Lidah"),
        TextKey::SculptInnerMouth => Some("Rongga Mulut"),
        TextKey::SculptEyes => Some("Mata"),
        TextKey::Size => Some("Ukuran"),
        TextKey::SculptXSymmetry => Some("Simetri X"),
        TextKey::SculptXSymmetryTooltip => Some("Menerapkan sapuan yang sama di kedua sisi"),
        TextKey::BackfaceProtection => Some("Pelindung Backface"),
        TextKey::AutoGroup => Some("Grup Otomatis"),
        TextKey::AutoGroupTooltip => Some("Mendeformasi hanya di dalam satu grup poligon"),
        TextKey::EyeTrackingAuto => Some("Otomatis"),
        TextKey::EyeGazeFreeze => Some("Bekukan jadi morph"),
        TextKey::EyeGazeFreezeTooltip => Some(
            "Mengunci arah pandang saat ini ke dalam morph, sehingga tab simpan mengekspor pose mata ini",
        ),
        TextKey::Reset => Some("Atur Ulang"),
        TextKey::SculptGrab => Some("Grab"),
        TextKey::SculptSmooth => Some("Haluskan"),
        TextKey::SculptPushPull => Some("Dorong · Tarik"),
        TextKey::ShortcutSculptGrab => Some("Seret"),
        TextKey::ShortcutSculptSmooth => Some("Shift + Seret"),
        TextKey::ShortcutSculptPushPull => Some("Ctrl / Alt + Seret"),
        TextKey::ResetSculpt => Some("Atur Ulang Deformasi"),
        #[cfg(test)]
        TextKey::SculptApplied => Some("Suntingan sculpt diterapkan"),
        TextKey::SculptFailed => Some("Operasi sculpt gagal"),
        TextKey::Ready => Some("Siap"),
        TextKey::Busy => Some("Penyuntingan terkunci selama proses"),
        TextKey::NeedScan => Some(
            "Buka dulu OBJ, GLB, atau FBX kepala pindaian, atau mulai dari look yang sudah ada di VaM",
        ),
        TextKey::ScanLoadFailed => Some("Berkas kepala itu tidak bisa dibaca"),
        TextKey::NeedEyeMorph => Some("Morph mata bawaan tidak kompatibel dengan G2 ini"),
        TextKey::ReviewEyePins => Some("Periksa pin kelopak mata yang merah"),
        TextKey::ResultUnavailable => Some("Fit wajah terlebih dahulu"),
        TextKey::MorphNotInLibrary => Some("Morph itu belum ada di pustaka; katalog masih dimuat."),
        TextKey::SculptNeedsBaseHead => Some("Lanjutkan dulu dengan kepala dasar di tab Buat."),
        TextKey::MorphUnavailable => Some("Tidak ada morph hasil yang kompatibel"),
        TextKey::AlignmentPending => Some("Muat kepala pindaian dan pilih folder VaM Anda"),
        TextKey::ScanLoaded => Some("OBJ, GLB, atau FBX kepala kustom dimuat"),
        TextKey::TemplateLoaded => Some("G2 dimuat"),
        TextKey::PinsReset => Some("Semua pin telah diatur ulang"),
        TextKey::ResultStale => Some("Ada perubahan, hasilnya perlu dibuat ulang"),
        TextKey::GenerationStarted => Some("Mem-fit wajah"),
        TextKey::Cancelling => Some("Membatalkan fit wajah"),
        TextKey::GenerationComplete => Some("Fit wajah selesai"),
        TextKey::GenerationCancelled => Some("Fit wajah dibatalkan"),
        TextKey::GenerationFailed => Some("Fit wajah gagal"),
        TextKey::ExportStarted => Some("Mengekspor"),
        TextKey::ExportComplete => Some("Ekspor selesai"),
        TextKey::ExportFailed => Some("Ekspor gagal"),
        TextKey::MorphNameHint => Some("Masukkan nama morph"),
        TextKey::MorphGroupHint => Some("Grup  (mis. Custom, Morphs/Morph Loader)"),
        TextKey::MorphRegionHint => Some("Region  (mis. Custom, Morphs/Body)"),
        TextKey::DirectEntry => Some("Ketik sendiri"),
        TextKey::VaMMetadataPoseMorph => Some("Daftarkan sebagai Pose Morph"),
        TextKey::VaMMetadataPoseMorphTooltip => Some("Ditangani bersama ekspresi dan pose"),
        TextKey::VaMMetadataBoneCorrection => Some("Koreksi Rangka"),
        TextKey::VaMMetadataBoneCorrectionTooltip => {
            Some("Memusatkan ulang pivot pada rangka yang telah diubah")
        }
        TextKey::NativeCorePending => Some("Integrasi inti fitting native sedang berjalan"),
        TextKey::MorphPreviewUpdated => Some("Pratinjau morph diperbarui"),
        TextKey::MorphsReset => Some("Morph diatur ulang"),
        TextKey::MorphsResetUndone => Some("Pengaturan ulang morph diurungkan"),
        TextKey::MorphEditUndone => Some("Penyesuaian morph diurungkan"),
        TextKey::MorphApplied => Some("Morph diterapkan"),
        TextKey::HelpPlace => Some("Pasang Pin"),
        TextKey::HelpMove => Some("Geser Pin"),
        TextKey::HelpDelete => Some("Hapus Pin"),
        TextKey::HelpXSymmetry => Some("Simetri X Pin"),
        TextKey::HelpSnapRotation => Some("Rotasi Snap"),
        TextKey::HelpDragZoom => Some("Zoom Seret"),
        TextKey::HelpOrbit => Some("Orbit"),
        TextKey::HelpPan => Some("Geser"),
        TextKey::HelpZoom => Some("Zoom"),
        TextKey::HelpLight => Some("Putar Cahaya"),
        TextKey::HelpFrameView => Some("Fokus ke Kepala"),
        TextKey::HelpUndo => Some("Urungkan"),
        TextKey::ShortcutPlace => Some("Klik Kiri"),
        TextKey::ShortcutMove => Some("Alt + Seret"),
        TextKey::ShortcutDelete => Some("Klik Kanan"),
        TextKey::ShortcutXSymmetry => Some("X"),
        TextKey::ShortcutSnapRotation => Some("Shift / Ctrl + Seret Putar"),
        TextKey::ShortcutDragZoom => Some("Ctrl + Seret Vertikal Tombol Tengah"),
        TextKey::ShortcutOrbit => Some("Seret Tombol Tengah"),
        TextKey::ShortcutPan => Some("Shift + Seret Tombol Tengah"),
        TextKey::ShortcutZoom => Some("Roda Mouse"),
        TextKey::ShortcutLight => Some("Shift + Seret Kanan"),
        TextKey::ShortcutFrameView => Some("F"),
        TextKey::HelpSnapView => Some("Snap Tampilan"),
        TextKey::ShortcutSnapView => Some("Alt + Seret Roda"),
        TextKey::HelpStandardViews => Some("Tampilan Standar"),
        TextKey::ShortcutStandardViews => Some("Numpad 1-9"),
        TextKey::ShortcutUndo => Some("Ctrl+Z"),
        TextKey::TemplatePending => Some("Pilih folder VaM Anda agar basis G2 disiapkan otomatis"),
        TextKey::ResultEmpty => Some("Tidak ada morph siap"),
        TextKey::ScaleLink => Some("Tautkan Skala"),
        TextKey::ScaleLinkTooltip => Some("Tautkan nilai skala X/Y/Z"),
        TextKey::CameraSettings => Some("Pengaturan Kamera"),
        TextKey::Perspective => Some("Perspektif"),
        TextKey::Orthographic => Some("Ortografis"),
        TextKey::Fov => Some("FOV"),
        TextKey::ResetCamera => Some("Atur Ulang Kamera"),
        TextKey::WindowMinimize => Some("Minimalkan"),
        TextKey::WindowMaximize => Some("Maksimalkan"),
        TextKey::WindowRestore => Some("Pulihkan"),
        TextKey::WindowClose => Some("Tutup"),
        TextKey::TooltipShow => Some("Tampilkan"),
        TextKey::TooltipHide => Some("Sembunyikan"),
        TextKey::TooltipLock => Some("Kunci"),
        TextKey::TooltipUnlock => Some("Buka kunci"),
        TextKey::AdjustEyeGaze => Some("Atur arah pandang"),
        TextKey::ScanLoading => Some("Memuat kepala pindaian"),
        TextKey::TemplateLoading => Some("Memuat templat G2"),
        TextKey::ResultLoading => Some("Memuat mesh hasil"),
        TextKey::VaMMorphPairLoading => Some("Memuat pasangan morph VaM"),
        TextKey::SystemLog => Some("Log sistem"),
        TextKey::Strength => Some("Kekuatan"),
        TextKey::StrengthShort => Some("Kuat"),
        TextKey::SculptBrushMove => Some("Grab"),
        TextKey::SculptBrushSmooth => Some("Haluskan"),
        TextKey::SculptBrushRestore => Some("Pulihkan"),
        TextKey::SculptBrushMoveTooltip => Some(
            "Menarik permukaan mengikuti penunjuk; kekuatan menentukan seberapa jauh ia mengikuti",
        ),
        TextKey::SculptBrushSmoothTooltip => Some("Menghaluskan detail permukaan"),
        TextKey::SculptBrushRestoreTooltip => Some("Mengembalikan ke arah bentuk dasar"),
        TextKey::TextureLayers => Some("Lapisan Tekstur"),
        TextKey::PackageSection => Some("Ekspor VAR"),
        TextKey::PackageCreatorHint => Some("Kreator"),
        TextKey::PackageNameHint => Some("Nama paket"),
        TextKey::PackageDescriptionHint => Some("Deskripsi"),
        TextKey::PackageCreditsHint => Some("Kredit"),
        TextKey::PackageInstructionsHint => Some("Petunjuk"),
        TextKey::PackageLinkHint => Some("Tautan dukungan"),
        TextKey::PackageSave => Some("Simpan VAR"),
        TextKey::PackageFromThisHead => Some("Buat dari kepala ini"),
        TextKey::PackageAddMorphs => Some("Tambah morph"),
        TextKey::PackageAddTextures => Some("Tambah tekstur"),
        TextKey::PackageNeedsVamRestart => Some("VaM harus dimulai ulang"),
        TextKey::PackageSavedTo => Some("Disimpan di"),
        TextKey::FieldRequired => Some("(wajib)"),
        TextKey::FieldOptional => Some("(opsional)"),
        TextKey::PackageSaveTo => Some("Simpan ke"),
        TextKey::PackageNothingToPack => {
            Some("Tidak ada yang dikemas. Aktifkan kepala ini atau tambahkan berkas.")
        }
        TextKey::PackageVersionMarker => Some("V"),
        TextKey::PackageCreatorRequired => Some("Nama kreator wajib diisi."),
        TextKey::PackageNameRequired => Some("Nama paket wajib diisi."),
        TextKey::PackageReplaceTitle => Some("VAR dengan nama ini sudah ada"),
        TextKey::PackageReplaceBody => {
            Some("Ganti? Scene yang merujuk versi ini akan membaca isi yang baru.")
        }
        TextKey::LicenseByTooltip => Some("cantumkan penulis"),
        TextKey::LicenseBySaTooltip => Some("cantumkan penulis / berbagi serupa"),
        TextKey::LicenseByNdTooltip => Some("cantumkan penulis / tanpa turunan"),
        TextKey::LicenseByNcTooltip => Some("cantumkan penulis / tanpa penggunaan komersial"),
        TextKey::LicenseByNcSaTooltip => {
            Some("cantumkan penulis / tanpa penggunaan komersial / berbagi serupa")
        }
        TextKey::LicenseByNcNdTooltip => {
            Some("cantumkan penulis / tanpa penggunaan komersial / tanpa turunan")
        }
        TextKey::LicensePcEaTooltip => Some("akses awal patron, tanpa redistribusi"),
        TextKey::AddTextureLayer => Some("Tambah gambar"),
        TextKey::AddG2UvTextureLayer => Some("Tambah peta UV"),
        TextKey::AddTextureLayerTooltip => {
            Some("Menempatkan foto wajah pada kepala dan menyuntingnya sebagai tekstur")
        }
        TextKey::AddG2UvTextureLayerTooltip => {
            Some("Menambahkan tekstur face G2 pada tata letak UV-nya sendiri dan menyuntingnya")
        }
        TextKey::AddTextureLayerHint => {
            Some("Tambahkan gambar wajah atau tekstur G2 dengan tombol +.")
        }
        TextKey::BakingTextures => Some("Mem-bake tekstur…"),
        TextKey::BakeTexturesForSave => Some("Bake Tekstur untuk Disimpan"),
        TextKey::TextureMetalRough => Some("Metallic / Roughness"),
        TextKey::TextureGlossSmooth => Some("Glossiness / Smoothness"),
        TextKey::LayerOpacity => Some("Opasitas lapisan"),
        TextKey::PinOpacity => Some("Opasitas pin"),
        TextKey::TransferPins => Some("Salin pin"),
        TextKey::TransferPinsTooltip => {
            Some("Salin pin yang Anda pasang ke gambar lapisan lainnya.")
        }
        TextKey::DeleteLayer => Some("Hapus lapisan"),
        TextKey::TextureNormalStrength => Some("Kekuatan normal"),
        TextKey::TextureInvertScalar => Some("Balik nilai skalar"),
        TextKey::BlendNormal => Some("Blend normal"),
        TextKey::BlendMultiply => Some("Multiply"),
        TextKey::BlendScreen => Some("Screen"),
        TextKey::BlendOverlay => Some("Overlay"),
        TextKey::MatchToneToLayerBelow => Some("Samakan tone dengan lapisan di bawah"),
        TextKey::ImageMirror => Some("Cermin kiri/kanan"),
        TextKey::ImageMirrorOff => Some("Mati"),
        TextKey::ImageMirrorToLeft => Some("Ke kiri"),
        TextKey::ImageMirrorToRight => Some("Ke kanan"),
        TextKey::ImageAdjustments => Some("Penyesuaian gambar"),
        TextKey::Exposure => Some("Eksposur"),
        TextKey::Contrast => Some("Kontras"),
        TextKey::Saturation => Some("Saturasi"),
        TextKey::Hue => Some("Hue"),
        TextKey::Temperature => Some("Suhu"),
        TextKey::TextureMaskPreviewTooltip => {
            Some("Tampilkan mask lapisan aktif sebagai hamparan merah tembus pandang")
        }
        TextKey::ClearLayerMask => Some("Bersihkan mask lapisan"),
        TextKey::ResetSourceRetouch => Some("Atur ulang retouch sumber"),
        TextKey::CloneSampleHint => Some("Alt-klik gambar sumber untuk memilih sampel clone."),
        TextKey::BakeBase => Some("Opsi bake"),
        TextKey::TransparentOverlay => Some("Bake lapisan saja"),
        TextKey::CurrentSkinBake => Some("Bake dengan kulit VaM"),
        TextKey::TransparentOverlayTooltip => {
            Some("Mem-bake gambar lapisan saja, tanpa preset kulit")
        }
        TextKey::CurrentSkinBakeTooltip => {
            Some("Mem-bake lapisan di atas preset kulit sebagai dasarnya")
        }
        TextKey::SkinPresetRequired => Some("Pilih preset kulit dulu di pengaturan Kulit"),
        TextKey::SkinSettings => Some("Pengaturan kulit"),
        TextKey::ScanHeadColor => Some("Warna kepala pindaian"),
        TextKey::G2HeadColor => Some("Warna kepala G2"),
        TextKey::MorphFilterAll => Some("Semua"),
        TextKey::MorphFilterActive => Some("Aktif"),
        TextKey::BrushDodge => Some("Dodge"),
        TextKey::BrushBurn => Some("Burn"),
        TextKey::BrushSaturate => Some("Tambah saturasi"),
        TextKey::BrushDesaturate => Some("Kurangi saturasi"),
        TextKey::ShowLayersOnly => Some("Hanya gambar lapisan"),
        TextKey::ShowLayersOnlyTooltip => {
            Some("Sembunyikan kulit VaM dan tampilkan hanya gambar lapisan")
        }
        TextKey::MatchToneTooltip => {
            Some("Menyamakan eksposur, saturasi, dan suhu dengan lapisan di bawah")
        }
        TextKey::ClearLayerMaskTooltip => Some("Hapus mask lapisan ini"),
        TextKey::TextureMetalRoughTooltip => {
            Some("Menyimpan peta dalam konvensi metallic/roughness")
        }
        TextKey::TextureGlossSmoothTooltip => {
            Some("Menyimpan peta dalam konvensi glossiness/smoothness milik VaM")
        }
        TextKey::TextureInvertScalarTooltip => {
            Some("Membalik nilai bila sumber memakai konvensi sebaliknya (roughness ↔ glossiness)")
        }
        TextKey::BaseWithoutSkin => Some("Lapisan"),
        TextKey::BaseWithoutSkinTooltip => Some(
            "Lapisan yang dilukis di atas warna solid, tanpa preset; tekstur yang diekspor tidak berubah",
        ),
        TextKey::SolidColorTooltip => Some("Hanya warna solid, tanpa tekstur"),
        TextKey::TextureSkinTooltip => Some("Lapisan yang dilukis di atas preset kulit VaM"),
        TextKey::BoundaryFeather => Some("Lembutkan tepi wajah"),
        TextKey::BoundaryFeatherTooltip => Some(
            "Hanya diffuse. Map seperti normal dan roughness tidak bisa diblend lewat alpha, jadi tepinya tetap keras",
        ),
        TextKey::Baking => Some("Mem-bake…"),
        TextKey::TextureToolPinPair => {
            Some("Pasangan Pin — tempatkan titik yang cocok pada wajah 3D dan gambar sumber")
        }
        TextKey::TextureToolMask => {
            Some("Mask — lukis untuk menampakkan; tahan Alt untuk menyembunyikan")
        }
        TextKey::TextureToolClone => {
            Some("Clone Stamp — Alt-klik sampel, lalu lukis untuk menyalinnya")
        }
        TextKey::TextureToolDodgeBurn => {
            Some("Dodge/Burn — mencerahkan; tahan Alt untuk menggelapkan")
        }
        TextKey::TextureToolSponge => {
            Some("Sponge — menambah saturasi; tahan Alt untuk menguranginya")
        }
        TextKey::SelectTextureLayer => Some("Pilih lapisan tekstur."),
        TextKey::LoadingTextureImage => Some("Memuat gambar…"),
        TextKey::ScanTextureAlignment => {
            Some("Tekstur pindaian mewarisi penyelarasan 3D hasil fit.")
        }
        TextKey::NoTextureImage => Some("Tidak ada gambar."),
        TextKey::MorphSavedReloadFemale => {
            Some("Tekan Reload Custom Morphs di tab Female Morphs VaM untuk memperbarui.")
        }
        TextKey::MorphSavedReloadMale => {
            Some("Tekan Reload Custom Morphs di tab Male Morphs VaM untuk memperbarui.")
        }
        TextKey::BootStarting => Some("Menyiapkan Vkit"),
        TextKey::BootFonts => Some("Memuat fon"),
        TextKey::BootGraphics => Some("Memulai grafis"),
        TextKey::BootSettings => Some("Memuat pengaturan"),
        TextKey::BootTemplate => Some("Memuat G2"),
        TextKey::BootWorkspace => Some("Menyiapkan ruang kerja"),
        TextKey::BootReady => Some("Membuka"),
        TextKey::StartupFailedTitle => Some("Vkit tidak dapat dijalankan"),
        TextKey::StartupFailedGraphics => Some(
            "Vkit menggambar dengan DirectX 12, dan grafis komputer ini tidak dapat menyediakannya. Perbarui driver grafis Anda, atau jalankan Vkit di komputer dengan GPU yang mendukung DirectX 12. Sesi desktop jarak jauh atau mesin virtual sering kali tidak punya tampilan DirectX 12 sendiri.",
        ),
        TextKey::StartupFailedOther => {
            Some("Vkit berhenti saat memulai dan tidak dapat membuka jendelanya.")
        }
        TextKey::StartupFailedLogHint => Some("Rincian kesalahannya ditulis di:"),
        TextKey::DialogOpenScan => Some("Buka OBJ, GLB, atau FBX kepala"),
        TextKey::DialogAddUvLayer => Some("Tambah lapisan tekstur G2 UV"),
        TextKey::DialogAddTextureLayer => Some("Tambah lapisan tekstur"),
        TextKey::DialogSaveMorphPair => Some("Simpan morph VaM VMI + VMB"),
        TextKey::DialogChooseVamFolder => Some("Pilih folder Virt-A-Mate"),
        _ => None,
    }
}
