use super::*;

impl AppState {
    pub(super) fn reset_relevant_cameras(&mut self) {
        let bounds_for_side = |state: &Self, side: MeshSide| {
            if state.active_tab == Tab::Alignment {
                state.alignment_head_bounds()
            } else {
                state.edit_side_head_bounds(side)
            }
        };
        match self.active_tab {
            Tab::Alignment | Tab::Edit => {
                if self.cameras_linked {
                    let authoritative = if self.workspace.template.is_some() {
                        MeshSide::Template
                    } else {
                        MeshSide::Scan
                    };
                    let bounds = bounds_for_side(self, authoritative);
                    match authoritative {
                        MeshSide::Scan => {
                            self.workspace
                                .scan_camera
                                .reset_view_with_default_fov(bounds);
                        }
                        MeshSide::Template => {
                            self.workspace
                                .template_camera
                                .reset_view_with_default_fov(bounds);
                        }
                    }
                    self.workspace.reconcile_linked_edit_cameras(authoritative);
                } else {
                    let scan_bounds = bounds_for_side(self, MeshSide::Scan);
                    let template_bounds = bounds_for_side(self, MeshSide::Template);
                    self.workspace
                        .scan_camera
                        .reset_view_with_default_fov(scan_bounds);
                    self.workspace
                        .template_camera
                        .reset_view_with_default_fov(template_bounds);
                }
            }
            stage @ (Tab::Morph | Tab::Texture | Tab::Hair | Tab::Result) => {
                let bounds = self.result_head_bounds();
                if let Some(camera) = self.active_stage_camera_mut(stage) {
                    camera.reset_view_with_default_fov(bounds);
                }
            }
        }
    }

    fn with_relevant_cameras(&mut self, mut op: impl FnMut(&mut crate::camera::TurntableCamera)) {
        match self.active_tab {
            Tab::Alignment | Tab::Edit => {
                if self.cameras_linked {
                    let authoritative = if self.workspace.template.is_some() {
                        MeshSide::Template
                    } else {
                        MeshSide::Scan
                    };
                    match authoritative {
                        MeshSide::Scan => op(&mut self.workspace.scan_camera),
                        MeshSide::Template => op(&mut self.workspace.template_camera),
                    }
                    self.workspace.reconcile_linked_edit_cameras(authoritative);
                } else {
                    op(&mut self.workspace.scan_camera);
                    op(&mut self.workspace.template_camera);
                }
            }
            stage => {
                if let Some(camera) = self.active_stage_camera_mut(stage) {
                    op(camera);
                }
            }
        }
    }

    pub(super) fn toggle_relevant_projection(&mut self) {
        let toggled = |current| match current {
            ProjectionMode::Perspective => ProjectionMode::Orthographic,
            ProjectionMode::Orthographic => ProjectionMode::Perspective,
        };
        self.with_relevant_cameras(|camera| {
            let mode = toggled(camera.projection_mode);
            camera.set_projection_mode(mode);
        });
    }

    pub(super) fn set_relevant_standard_view(&mut self, view: StandardView) {
        self.with_relevant_cameras(|camera| camera.look_from_standard_view(view));
    }

    pub(super) fn set_relevant_fov(&mut self, value: f32) {
        self.with_relevant_cameras(|camera| {
            camera.set_fov_y_degrees_with_dolly_compensation(value);
        });
    }
}
