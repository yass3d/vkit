use super::TextKey;

pub(super) const fn label(key: TextKey) -> Option<&'static str> {
    #[allow(unreachable_patterns, reason = "the fallback outlives today's key set")]
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
        TextKey::HairStage => Some("Rambut"),
        TextKey::HairUnavailable => Some("Selesaikan hasil sculpt sebelum menyunting rambut"),
        TextKey::HairPartsPanel => Some("Bagian rambut"),
        TextKey::AddHairPart => Some("Tambah bagian rambut"),
        TextKey::HairScalpMeshCrowded => {
            Some("Sebagian helai tidak mendapat tempat pada mesh kulit kepala baru dan dihapus")
        }
        TextKey::HairScalpMissing => {
            Some("Mesh kulit kepala tidak tersedia; buka root VaM dan pindai ulang")
        }
        TextKey::HairShowPoints => Some("Tampilkan titik"),
        TextKey::HairShowPointsHint => Some(
            "Menampilkan titik tanam di kulit kepala. Matikan untuk melihat rambutnya saja; kuas tetap bekerja.",
        ),
        TextKey::HairPartTint => Some("Warnai bagian"),
        TextKey::HairPartTintHint => Some(
            "Memberi tiap bagian rona berbeda di viewport agar mudah dibedakan. Hanya tampilan; ekspor tidak berubah.",
        ),
        TextKey::HairSettingsPanel => Some("Pengaturan rambut"),
        TextKey::HairCreateFirst => Some("Buat rambut dulu."),
        TextKey::HairHideStrands => Some("Sembunyikan rambut"),
        TextKey::HairHideStrandsHint => Some(
            "Melepas rambut yang tumbuh agar untai utama terbaca sendiri. Menyalakannya menyalakan untai juga.",
        ),
        TextKey::HairShowStreams => Some("Tampilkan untai utama"),
        TextKey::HairShowStreamsHint => {
            Some("Menggambar untai yang Anda edit, di atas rambut yang tumbuh darinya.")
        }
        TextKey::HairViewportPhysics => Some("Fisika viewport"),
        TextKey::HairViewportPhysicsHint => {
            Some("Melihat pratinjau fisika di layar. Tidak ada hubungannya dengan ekspor.")
        }
        TextKey::HairMirrorEdit => Some("Edit cermin"),
        TextKey::HairMirrorEditHint => Some("Kuas mengedit kedua sisi sekaligus secara simetris."),
        TextKey::HairAutoPart => Some("Part otomatis"),
        TextKey::HairAutoPartHint => {
            Some("Kuas hanya mengedit part yang dikenali di bawah kursor, bukan layer aktif.")
        }
        TextKey::HairExportSection => Some("Simpan rambut"),
        TextKey::HairExportBothSexes => Some("Keduanya"),
        TextKey::HairExportRescanNotice => {
            Some("Klik VaM - Hair - Rescan Files agar muncul di daftar")
        }
        TextKey::HairExportOverwroteNotice => {
            Some("Thumbnail yang ditimpa tampil setelah Hard Reset atau restart VaM")
        }
        TextKey::HairSimToggleHint => Some("Menjalankan fisika rambut di sini dan dalam gim."),
        TextKey::HairCollisionToggleHint => Some("Mencegah rambut menembus kepala dan tubuh."),
        TextKey::HairStyleJoints => Some("Sendi gaya"),
        TextKey::HairStyleJointsHint => Some(
            "Mengikat helai berdekatan agar gaya rambut mempertahankan bentuk di game -- yang diskalakan Cling. Untuk rambut panjang atau terikat; memperbesar file.",
        ),
        TextKey::HairThumbnailPrompt => Some("Atur sudut lalu klik untuk memotret"),
        TextKey::HairThumbnailShoot => Some("Ambil"),
        TextKey::HairThumbnailSkip => Some("Lewati"),
        TextKey::HairThumbnailSaved => Some("Thumbnail tersimpan"),
        TextKey::HairThumbnailFailed => Some("Thumbnail gagal disimpan"),
        TextKey::HairGameOnly => Some(
            "Pengaturan fisika: viewport tidak menjalankan simulasi, jadi hanya berlaku di VaM. Diekspor bersama gaya.",
        ),
        TextKey::HairPartVisible => Some("Tampilkan atau sembunyikan bagian rambut ini"),
        TextKey::RemoveHairPart => Some("Hapus bagian rambut"),
        TextKey::HairRenamePart => Some("Klik untuk mengganti nama"),
        TextKey::HairToolPlant => Some("Tanam"),
        TextKey::HairToolErase => Some("Hapus"),
        TextKey::HairToolGrow => Some("Tumbuhkan (Alt memendekkan)"),
        TextKey::HairToolComb => Some("Sisir"),
        TextKey::HairToolPlantHint => Some("Menanam helai pada titik kulit kepala di bawah kuas"),
        TextKey::HairToolGrowHint => {
            Some("Memanjangkan helai di bawah kuas; tahan Alt untuk memendekkan")
        }
        TextKey::HairToolEraseHint => Some("Menghapus helai di bawah kuas"),
        TextKey::HairToolCombHint => Some("Menyisir helai di bawah kuas; akarnya tetap di tempat"),
        TextKey::HairToolPinch => Some("Kumpulkan"),
        TextKey::HairToolPinchHint => Some("Mengumpulkan helai di bawah kuas. Alt menyebarkannya"),
        TextKey::HairToolCut => Some("Potong"),
        TextKey::HairToolCutHint => Some("Memotong helai di tempat kuas melintasinya"),
        TextKey::HairToolPuff => Some("Mengembang"),
        TextKey::HairToolPuffHint => Some("Menegakkan helai di bawah kuas. Alt merebahkannya"),
        TextKey::HairToolVertex => Some("Titik"),
        TextKey::HairToolVertexHint => {
            Some("Pegang satu sendi pada helai dan pindahkan; panjangnya tetap")
        }
        TextKey::HairCopySettings => Some("Salin semua"),
        TextKey::HairPasteSettings => Some("Tempel semua"),
        TextKey::HairGroupPerformance => Some("Kinerja"),
        TextKey::HairGroupPhysics => Some("Fisika"),
        TextKey::HairGroupStiffness => Some("Kekakuan"),
        TextKey::HairGroupShape => Some("Bentuk"),
        TextKey::HairGroupCurl => Some("Ikal"),
        TextKey::HairGroupLook => Some("Tampilan"),
        TextKey::HairColorScalp => Some("Kulit"),
        TextKey::HairColorRoot => Some("Akar"),
        TextKey::HairColorTip => Some("Ujung"),
        TextKey::HairColorSpecular => Some("Kilau"),
        TextKey::HairScalpMask => Some("Tekstur kulit kepala"),
        TextKey::HairScalpMaskBuiltIn => Some("Bawaan"),
        TextKey::HairScalpMaskCustom => Some("Dari berkas..."),
        TextKey::HairExport => Some("Ekspor ke VaM"),
        TextKey::HairExportName => Some("Nama item"),
        TextKey::HairExportCreator => Some("Pembuat"),
        TextKey::HairExportNeedsMetadata => Some("Isi nama dan pembuat sebelum mengekspor"),
        TextKey::HairOverwriteTitle => Some("Ganti gaya ini?"),
        TextKey::HairOverwriteBody => {
            Some("Gaya dengan nama dan pembuat ini sudah ada. Mengekspor akan mengganti berkasnya.")
        }
        TextKey::HairOverwriteProceed => Some("Ganti"),
        TextKey::HairExportDone => Some("Item rambut tersimpan"),
        TextKey::HairExportFailed => Some("Ekspor rambut gagal"),
        TextKey::HairExportNeedsPart => Some("Pilih dulu bagian rambut yang berisi helai"),
        TextKey::HairExportNeedsVaMRoot => Some("Atur root VaM sebelum mengekspor"),
        TextKey::HairMirrorPart => Some("Cerminkan bagian ke sisi lain"),
        TextKey::HairDuplicatePart => Some("Duplikasi bagian"),
        TextKey::HairSegmentsShort => Some("Seg"),
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
        TextKey::ScanOptions => Some("Opsi pindaian"),
        TextKey::RestoreNeckEars => Some("Pulihkan leher & telinga"),
        TextKey::RestoreNeckEarsTooltip => Some(
            "Menjaga telinga, tengkuk, dan belakang kepala pada basis G2, membaurkan hasil fit hanya ke arah wajah",
        ),
        TextKey::RestoreNeckEarsDeferred => {
            Some("Ada goresan pahatan; perubahan berlaku pada generasi berikutnya")
        }
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
        TextKey::ResetAllPins => Some("Atur Ulang Semua Pin"),
        TextKey::ResetAll => Some("Atur ulang semua"),
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
        TextKey::TextureToolNeedsPins => Some("Pasang pin dulu untuk memakainya"),
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
        TextKey::HairSearch => Some("Cari Rambut"),
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
        TextKey::SettingsShortcuts => Some("Pintasan"),
        TextKey::ShortcutsCapturing => Some("Tekan…"),
        TextKey::ShortcutsResetAll => Some("Setel ulang semua pintasan"),
        TextKey::ShortcutGroupSystem => Some("Di mana saja"),
        TextKey::ShortcutGroupAlignment => Some("Menempatkan kepala"),
        TextKey::ShortcutGroupSculpt => Some("Kuas pahat"),
        TextKey::ShortcutGroupHair => Some("Kuas rambut"),
        TextKey::ShortcutGroupView => Some("Tampilan kamera"),
        TextKey::ShortcutGroupNavigation => Some("Ganti tab"),
        TextKey::ShortcutViewReset => Some("Atur ulang kamera"),
        TextKey::ShortcutViewProjection => Some("Perspektif / ortografis"),
        TextKey::ShortcutViewFront => Some("Tampak depan"),
        TextKey::ShortcutViewLeftSide => Some("Tampak kiri"),
        TextKey::ShortcutViewRightSide => Some("Tampak kanan"),
        TextKey::ShortcutViewTop => Some("Tampak atas"),
        TextKey::ShortcutViewBottom => Some("Tampak bawah"),
        TextKey::ShortcutViewFrontUpperLeft => Some("Depan atas kiri"),
        TextKey::ShortcutViewFrontUpperRight => Some("Depan atas kanan"),
        TextKey::ShortcutViewFrontLowerLeft => Some("Depan bawah kiri"),
        TextKey::ShortcutViewFrontLowerRight => Some("Depan bawah kanan"),
        TextKey::ShortcutTabFaceMatch => Some("Tab pencocokan wajah"),
        TextKey::ShortcutTabDetail => Some("Tab detail"),
        TextKey::ShortcutTabTexture => Some("Tab tekstur"),
        TextKey::ShortcutTabHair => Some("Tab rambut"),
        TextKey::ShortcutTabSave => Some("Tab simpan"),
        TextKey::ShortcutGroupLists => Some("Lapisan dan bagian"),
        TextKey::ShortcutGroupTexture => Some("Kanvas tekstur"),
        TextKey::ReferenceImages => Some("Referensi"),
        TextKey::ReferenceImagesEmpty => Some("Tidak ada gambar referensi"),
        TextKey::ReferenceImageAdd => Some("Tambah gambar"),
        TextKey::ReferenceImageRemove => Some("Hapus"),
        TextKey::ReferenceImageRaise => Some("Maju"),
        TextKey::ReferenceImageLower => Some("Mundur"),
        TextKey::ShortcutTextureCanvasPan => Some("Geser kanvas (tahan)"),
        TextKey::ShortcutDiagnosticLogCopy => Some("Salin log"),
        TextKey::ShortcutLightRotate => Some("Putar cahaya (tahan)"),
        TextKey::ShortcutLayerHide => Some("Sembunyikan lapisan"),
        TextKey::ShortcutLayerUnhideAll => Some("Tampilkan semua lapisan"),
        TextKey::ShortcutLayerIsolate => Some("Sembunyikan lapisan lain"),
        TextKey::ShortcutLayerLocalView => Some("Tampilkan ini saja (alih)"),
        TextKey::ShortcutLayerInvertSelection => Some("Balik pilihan"),
        TextKey::ShortcutSculptSmoothHold => Some("Tahan untuk menghaluskan"),
        TextKey::ShortcutSculptInflateHold => Some("Tahan untuk mengembang"),
        TextKey::ShortcutSculptAlternateHold => Some("Tahan untuk membalik"),
        TextKey::ShortcutHairSmoothHold => Some("Tahan untuk menghaluskan rambut"),
        TextKey::ShortcutHairInvertHold => Some("Tahan untuk membalik alat rambut"),
        TextKey::ShortcutTextureInvertHold => Some("Tahan untuk membalik alat tekstur"),
        TextKey::ShortcutListAddToSelection => Some("Tahan untuk menambah ke pilihan"),
        TextKey::ShortcutListSolo => Some("Tahan untuk menampilkan satu saja"),
        TextKey::ShortcutsExport => Some("Simpan peta tombol ke berkas"),
        TextKey::ShortcutsImport => Some("Muat berkas peta tombol"),
        TextKey::ShortcutsTaken => Some("Tindakan lain sudah memakainya"),
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
        TextKey::SettingsLightingPreset => Some("Preset"),
        TextKey::SettingsExposure => Some("Eksposur"),
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
        TextKey::HairNeedsFinishedHead => Some("Rambut dipakai pada kepala yang sudah jadi."),
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
        TextKey::AppearanceLayers => Some("Lapisan penampilan"),
        TextKey::AddAppearanceLayer => Some("Tambahkan penampilan terpilih"),
        TextKey::AppearanceLayersEmpty => Some("Belum ada lapisan"),
        TextKey::AppearanceLayerRaise => Some("Naik"),
        TextKey::AppearanceLayerLower => Some("Turun"),
        TextKey::LookFind => Some("Muat Look"),
        TextKey::CameraTrackballArmed => {
            Some("Trackball · seret untuk memutar bebas · klik atau R untuk keluar")
        }
        TextKey::HelpTrackball => Some("Rotasi trackball"),
        TextKey::ShortcutTrackball => Some("R"),
        TextKey::SplitModelViewTooltip => Some("Lihat dari dua sudut — bagi tampilan"),
        TextKey::SplitModelViewName => Some("Tampilan terbagi"),
        TextKey::HelpLevelRoll => Some("Ratakan cakrawala"),
        TextKey::ShortcutLevelRoll => Some("Alt+R"),
        TextKey::RecoveryTitle => Some("Sesi terakhir berakhir tak terduga"),
        TextKey::RecoveryBody => Some(
            "Morph, sculpting, dan lapisan tekstur Anda tersimpan tepat sebelum berhenti. Kembalikan?",
        ),
        TextKey::RecoveryRestore => Some("Pulihkan"),
        TextKey::RecoveryDiscard => Some("Mulai baru"),
        TextKey::RecoveryRestored => Some("Sesi sebelumnya dipulihkan."),
        TextKey::RecoveryLookMissing => Some(
            "Look sesi itu belum terdaftar — tekstur dipulihkan, dan suntingan akan diterapkan saat look dipilih.",
        ),
        TextKey::SettingsBrushSweepGroup => Some("Gestur ukuran kuas"),
        TextKey::BrushSweepBlender => Some("Blender"),
        TextKey::BrushSweepZBrush => Some("ZBrush"),
        TextKey::BrushSweepTooltip => {
            Some("Blender dikunci dengan klik atau tekan ulang; ZBrush saat tombol dilepas")
        }
        TextKey::HairPackageStyle => Some("Paketkan sebagai .var"),
        TextKey::HairPackageDone => Some("Rambut dipaketkan"),
        TextKey::HairPackageFailed => Some("Gagal memaketkan"),
        TextKey::HairPackageNeedsInstall => Some("Pasang gaya rambut dulu"),
        TextKey::DialogOpenHeadPreset => Some("Buka preset kepala"),
        TextKey::FindHeadPreset => Some("Cari preset…"),
        TextKey::SettingsVignetteIntensity => Some("Kekuatan"),
        TextKey::SettingsVignetteSmoothness => Some("Kelembutan"),
        TextKey::SettingsVignetteRoundness => Some("Kebulatan"),
        TextKey::SculptNotCarried => Some("Sculpting tidak bisa mengikuti look ini"),
        TextKey::SettingsToneCurve => Some("Kurva tone"),
        TextKey::SettingsToneCurveTooltip => Some("Menyesuaikan bayangan dan sorotan"),
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
        TextKey::SettingsGraphicsQuality => Some("Kualitas"),
        TextKey::SettingsMsaa => Some("MSAA"),
        TextKey::SettingsMsaaTooltip => Some(
            "Menghaluskan tepi bergerigi sesuai jumlah sampel yang dipilih. Makin tinggi makin bersih dan berat.",
        ),
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
        TextKey::ScanUnloaded => Some("Berkas kepala dilepas; basis G2 siap disunting"),
        TextKey::PinPairsMismatched => Some("Jumlah pin tidak cocok; pin tanpa pasangan"),
        TextKey::UnloadScanTooltip => Some("Lepas berkas kepala ini"),
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
        TextKey::HelpCameraProjection => Some("Perspektif / Ortho"),
        TextKey::ShortcutStandardViews => Some("Numpad 1-9"),
        TextKey::ShortcutCameraProjection => Some("Numpad ."),
        TextKey::ShortcutUndo => Some("Ctrl+Z"),
        TextKey::HelpRedo => Some("Ulangi"),
        TextKey::ShortcutRedo => Some("Ctrl+Y"),
        TextKey::HairPresetSection => Some("Praatur rambut"),
        TextKey::HairPresetLoad => Some("Muat"),
        TextKey::HairToolPick => Some("Pilih lapisan"),
        TextKey::HairToolPickHint => Some(
            "Klik untai di viewport untuk mengaktifkan lapisannya. Kuas menunggu klik berikutnya.",
        ),
        TextKey::HelpHairPick => Some("Pilih lapisan di bawah kursor"),
        TextKey::ShortcutHairPick => Some("V, atau Ctrl + klik dengan kuas apa pun"),
        TextKey::HelpHairSmooth => Some("Haluskan alih-alih menyisir"),
        TextKey::ShortcutHairSmooth => Some("Shift saat menyisir"),
        TextKey::HairGroupScalp => Some("Kulit kepala"),
        TextKey::HairScalpAlpha => Some("Masker kulit kepala"),
        TextKey::HairScalpPanel => Some("Kulit kepala"),
        TextKey::HairScalpMesh => Some("Mesh kulit kepala"),
        TextKey::HairScalpCreate => Some("Buat kulit kepala"),
        TextKey::HairScalpAbsent => Some("Gaya ini belum punya lapisan kulit kepala."),
        TextKey::HistoryBranchTitle => Some("Langkah setelah ini akan hilang"),
        TextKey::HistoryBranchProceed => Some("Tetap edit"),
        TextKey::DoNotShowAgain => Some("Jangan tampilkan lagi"),
        TextKey::SurfaceBaseUnblended => {
            Some("Sebagian peta PBR tidak dapat dipadukan dengan kulit VaM")
        }
        TextKey::DetailEditRedone => Some("Suntingan yang dibatalkan diterapkan lagi"),
        TextKey::ShortcutBrushSmaller => Some("Perkecil kuas"),
        TextKey::ShortcutBrushLarger => Some("Perbesar kuas"),
        TextKey::ShortcutBrushSizeDrag => Some("Ukuran kuas"),
        TextKey::ShortcutBrushStrengthDrag => Some("Kekuatan kuas"),
        TextKey::ShortcutStencilCancel => Some("Batalkan penempatan stensil"),
        TextKey::ShortcutViewOrbit => Some("Orbit tampilan"),
        TextKey::ShortcutViewPan => Some("Geser tampilan"),
        TextKey::ShortcutViewDolly => Some("Dekatkan dan jauhkan tampilan"),
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
        TextKey::SculptBrushMask => Some("Masker"),
        TextKey::SculptBrushMaskTooltip => Some(
            "Mengikis lapisan penampilan terpilih agar yang di bawah tampak. Alt mengembalikan",
        ),
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

pub(super) fn hair_label(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some("Jumlah rambut"),
        "curveDensity" => Some("Kerapatan kurva"),
        "iterations" => Some("Iterasi"),
        "simulationEnabled" => Some("Fisika"),
        "collisionEnabled" => Some("Tumbukan"),
        "weight" => Some("Berat"),
        "drag" => Some("Hambatan udara"),
        "gravityMultiplier" => Some("Pengali gravitasi"),
        "collisionRadius" => Some("Radius tumbukan"),
        "collisionRadiusRoot" => Some("Radius tumbukan (akar)"),
        "friction" => Some("Gesekan"),
        "bendResistance" => Some("Ketahanan tekuk"),
        "rootRigidity" => Some("Kekakuan akar"),
        "mainRigidity" => Some("Kekakuan tengah"),
        "tipRigidity" => Some("Kekakuan ujung"),
        "jointRigidity" => Some("Kekakuan sambungan"),
        "rigidityRolloffPower" => Some("Peluruhan kekakuan"),
        "cling" => Some("Kohesi gaya"),
        "clingRolloff" => Some("Peluruhan kohesi"),
        "snap" => Some("Kancing"),
        "usePaintedRigidity" => Some("Pakai kekakuan yang dilukis"),
        "width" => Some("Ketebalan"),
        "length1" => Some("Panjang 1"),
        "length2" => Some("Panjang 2"),
        "length3" => Some("Panjang 3"),
        "maxSpread" => Some("Sebaran maksimum"),
        "spreadRoot" => Some("Sebaran (akar)"),
        "spreadMid" => Some("Sebaran (tengah)"),
        "spreadTip" => Some("Sebaran (ujung)"),
        "spreadMidpoint" => Some("Titik tengah sebaran"),
        "spreadCurvePower" => Some("Kurva sebaran"),
        "curlScale" => Some("Ukuran ikal"),
        "curlFrequency" => Some("Frekuensi ikal"),
        "curlScaleRandomness" => Some("Acak ukuran ikal"),
        "curlFrequencyRandomness" => Some("Acak frekuensi ikal"),
        "curlRoot" => Some("Ikal (akar)"),
        "curlMid" => Some("Ikal (tengah)"),
        "curlTip" => Some("Ikal (ujung)"),
        "curlMidpoint" => Some("Titik tengah ikal"),
        "curlCurvePower" => Some("Kurva ikal"),
        "curlX" => Some("Ikal X"),
        "curlY" => Some("Ikal Y"),
        "curlZ" => Some("Ikal Z"),
        "curlNormalAdjust" => Some("Koreksi normal ikal"),
        "curlAllowReverse" => Some("Izinkan arah terbalik"),
        "curlAllowFlipAxis" => Some("Izinkan sumbu terbalik"),
        "Alpha Adjust" => Some("Kelegapan kulit kepala"),
        "Diffuse Color" => Some("Warna difus kulit kepala"),
        "Gloss" => Some("Kilau kulit kepala"),
        "Specular Intensity" => Some("Spekular kulit kepala"),
        "Global Illumination Filter" => Some("Pencahayaan global kulit kepala"),
        "Diffuse Texture Offset" => Some("Geser tekstur kulit kepala"),
        "rootColor" => Some("Warna akar"),
        "tipColor" => Some("Warna ujung"),
        "specularColor" => Some("Warna spekular"),
        "colorRolloff" => Some("Peluruhan warna akar→ujung"),
        "randomColorPower" => Some("Kekuatan warna acak"),
        "randomColorOffset" => Some("Geser warna acak"),
        "primarySpecularSharpness" => Some("Kilau primer"),
        "secondarySpecularSharpness" => Some("Kilau sekunder"),
        "specularShift" => Some("Geser kilau sekunder"),
        "diffuseSoftness" => Some("Kelembutan difus"),
        "fresnelPower" => Some("Kekuatan Fresnel"),
        "fresnelAttenuation" => Some("Peluruhan Fresnel"),
        "IBLFactor" => Some("Cahaya tak langsung"),
        "normalRandomize" => Some("Acak normal"),
        _ => None,
    }
}

pub(super) fn hair_hint(key: &str) -> Option<&'static str> {
    match key {
        "hairMultiplier" => Some(
            "Berapa untai tumbuh di antara setiap tiga pemandu. Inilah rupa rambutnya, sekaligus sebagian besar biayanya.",
        ),
        "curveDensity" => Some(
            "Titik yang digambar sepanjang tiap untai. Lebih banyak berarti lebih halus dan lebih lambat; bentuknya terbaca jauh sebelum angkanya tinggi.",
        ),
        "iterations" => Some(
            "Lintasan solver per bingkai. Lebih banyak menahan kendala lebih erat saat gerakan cepat.",
        ),
        "simulationEnabled" => Some(
            "Apakah bagian ini bergerak di dalam game. Tidak ada hubungannya dengan sakelar viewport.",
        ),
        "collisionEnabled" => Some("Apakah untai bagian ini didorong keluar dari tubuh."),
        "weight" => Some(
            "Seberapa berat untai terjuntai. Menskalakan gravitasi sekaligus ketahanannya digerakkan.",
        ),
        "drag" => Some(
            "Seberapa kuat udara menahan untai. Tinggi berarti cepat tenang dan sedikit berayun.",
        ),
        "gravityMultiplier" => {
            Some("Gravitasi pada bagian ini, sebagai kelipatan gravitasi adegan.")
        }
        "collisionRadius" => {
            Some("Ketebalan untai saat bertumbukan, sepanjang seluruh panjangnya.")
        }
        "collisionRadiusRoot" => Some("Ketebalan di dekat akar, bila harus berbeda dari sisanya."),
        "friction" => Some("Seberapa kuat untai mencengkeram yang disentuhnya alih-alih meluncur."),
        "bendResistance" => Some("Seberapa kuat untai menolak ditekuk dari garis lurus."),
        "rootRigidity" => Some("Seberapa kuat ujung akar menahan bentuk yang ditata."),
        "mainRigidity" => Some("Seberapa kuat bagian tengah menahan bentuk yang ditata."),
        "tipRigidity" => Some("Seberapa kuat ujung menahan bentuk yang ditata."),
        "jointRigidity" => {
            Some("Kekakuan sambungan gaya: pegas yang membuat ekor kuda tetap ekor kuda.")
        }
        "rigidityRolloffPower" => Some(
            "Bagaimana ketiga kekakuan berbaur sepanjang untai. Makin tinggi, nilai akar terbawa makin jauh.",
        ),
        "cling" => Some("Seberapa kuat sambungan gaya menarik untai saling mendekat."),
        "clingRolloff" => Some(
            "Seberapa jauh kohesi memudar sepanjang untai. Makin tinggi makin terkurung dekat akar.",
        ),
        "snap" => Some("Seberapa teguh tiap ruas menahan panjang yang dibuatkan untuknya."),
        "usePaintedRigidity" => {
            Some("Ambil kekakuan dari peta yang dilukis, bukan dari tiga penggeser.")
        }
        "width" => Some(
            "Ketebalan gambar sebuah untai. Penggeser VaM sendiri juga berhenti di sepersepuluh milimeter.",
        ),
        "length1" => Some("Panjang untai terdekat ke pemandu pertama segitiganya."),
        "length2" => Some("Panjang untai terdekat ke pemandu kedua segitiganya."),
        "length3" => Some("Panjang untai terdekat ke pemandu ketiga segitiganya."),
        "maxSpread" => Some(
            "Batas atas seberapa jauh untai boleh menyimpang dari pemandunya, bukan penguat. Begitu melampaui jarak antar pemandu, menaikkannya tidak berpengaruh.",
        ),
        "spreadRoot" => {
            Some("Seberapa bebas untai memisah di akar. Rendah mengumpulkannya ke arah pemandu.")
        }
        "spreadMid" => Some("Seberapa bebas untai memisah di tengah."),
        "spreadTip" => Some("Seberapa bebas untai memisah di ujung."),
        "spreadMidpoint" => Some("Dari titik mana pada untai sebaran tengah berlaku."),
        "spreadCurvePower" => {
            Some("Seberapa tajam sebaran berbelok antara akar, tengah dan ujung.")
        }
        "curlScale" => Some("Lebar ikal: jari-jari lingkarannya, bukan panjangnya."),
        "curlFrequency" => Some("Putaran per sentimeter untai."),
        "curlScaleRandomness" => Some("Seberapa besar lebar ikal berbeda antar untai."),
        "curlFrequencyRandomness" => Some("Seberapa besar laju putaran berbeda antar untai."),
        "curlRoot" => Some("Seberapa banyak ikal sampai ke akar."),
        "curlMid" => Some("Seberapa banyak ikal sampai ke tengah."),
        "curlTip" => Some("Seberapa banyak ikal sampai ke ujung."),
        "curlMidpoint" => Some("Dari titik mana pada untai ikal tengah berlaku."),
        "curlCurvePower" => Some("Seberapa tajam ikal berbelok antara akar, tengah dan ujung."),
        "curlX" => {
            Some("Perpindahan ikal sepanjang X. Vektor yang diputar, bukan sumbu untuk melingkar.")
        }
        "curlY" => Some("Perpindahan ikal sepanjang Y."),
        "curlZ" => Some("Perpindahan ikal sepanjang Z."),
        "curlNormalAdjust" => Some(
            "Ditambahkan ke geseran ikal sepanjang normal. Ia mengangkat lilitan dari kepala, bukan memiringkannya.",
        ),
        "curlAllowReverse" => Some(
            "Biarkan sebagian untai melingkar terbalik, agar kepala berikal tidak terbaca sebagai satu untai berulang.",
        ),
        "curlAllowFlipAxis" => {
            Some("Biarkan sebagian untai melingkar pada sumbu terbalik, dengan alasan yang sama.")
        }
        "Alpha Adjust" => Some(
            "Seberapa legap tudung kulit kepala. Kebanyakan bagian memakai tudung milik orang lain dan menahannya di paling bawah agar tersembunyi; naikkan hanya bila belahan harus memperlihatkan kulit kepala.",
        ),
        "Diffuse Color" => Some(
            "Warna difus tudung itu sendiri. VaM mengirimkannya putih, agar lembar kulit di bawahnya tembus pandang.",
        ),
        "Gloss" => Some("Seberapa berkilau tudung di tempat ia terlihat."),
        "Specular Intensity" => Some("Seberapa kuat tudung menangkap kilau."),
        "Global Illumination Filter" => Some("Seberapa banyak cahaya sekitar yang diserap tudung."),
        "Diffuse Texture Offset" => {
            Some("Ditambahkan ke warna difus tudung. Meski namanya begitu, ini bukan nomor lembar.")
        }
        "rootColor" => Some("Warna di akar."),
        "tipColor" => Some("Warna di ujung."),
        "specularColor" => Some("Warna kilau."),
        "colorRolloff" => Some("Bagaimana warna akar beralih ke warna ujung sepanjang untai."),
        "randomColorPower" => Some("Seberapa kuat warna antar untai berbeda."),
        "randomColorOffset" => {
            Some("Ke arah mana perbedaan itu condong, lebih terang atau lebih gelap.")
        }
        "primarySpecularSharpness" => {
            Some("Seberapa rapat kilau utama. Makin tinggi makin mengilap.")
        }
        "secondarySpecularSharpness" => {
            Some("Seberapa rapat kilau kedua: yang berwarna dan ikut bergerak bersama rambut.")
        }
        "specularShift" => {
            Some("Seberapa jauh kilau kedua duduk dari yang pertama sepanjang untai.")
        }
        "diffuseSoftness" => Some(
            "Seberapa lembut cahaya membungkus untai alih-alih hanya menyinari sisi yang menghadapnya.",
        ),
        "fresnelPower" => {
            Some("Seberapa tajam tepi menyala di tempat rambut berpaling dari pandangan.")
        }
        "fresnelAttenuation" => Some("Seberapa kuat penyalaan tepi itu."),
        "IBLFactor" => Some("Seberapa banyak cahaya sekitar adegan yang diterima rambut."),
        "normalRandomize" => Some(
            "Seberapa besar normal bayangan tiap untai digoyang, agar tetangga tidak berbayang serupa.",
        ),
        _ => None,
    }
}
