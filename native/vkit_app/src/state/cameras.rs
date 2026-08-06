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
            Tab::Morph | Tab::Texture | Tab::Result => {
                let bounds = self.result_head_bounds();
                self.workspace
                    .result_camera
                    .reset_view_with_default_fov(bounds);
            }
        }
    }

    pub(super) fn toggle_relevant_projection(&mut self) {
        let toggled = |current| match current {
            ProjectionMode::Perspective => ProjectionMode::Orthographic,
            ProjectionMode::Orthographic => ProjectionMode::Perspective,
        };
        match self.active_tab {
            Tab::Alignment | Tab::Edit => {
                if self.cameras_linked {
                    let authoritative = if self.workspace.template.is_some() {
                        MeshSide::Template
                    } else {
                        MeshSide::Scan
                    };
                    match authoritative {
                        MeshSide::Scan => {
                            let mode = toggled(self.workspace.scan_camera.projection_mode);
                            self.workspace.scan_camera.set_projection_mode(mode);
                        }
                        MeshSide::Template => {
                            let mode = toggled(self.workspace.template_camera.projection_mode);
                            self.workspace.template_camera.set_projection_mode(mode);
                        }
                    }
                    self.workspace.reconcile_linked_edit_cameras(authoritative);
                } else {
                    let scan = toggled(self.workspace.scan_camera.projection_mode);
                    let template = toggled(self.workspace.template_camera.projection_mode);
                    self.workspace.scan_camera.set_projection_mode(scan);
                    self.workspace.template_camera.set_projection_mode(template);
                }
            }
            stage => {
                if let Some(camera) = self.workspace.stage_camera_mut(stage) {
                    let mode = toggled(camera.projection_mode);
                    camera.set_projection_mode(mode);
                }
            }
        }
    }

    pub(super) fn set_relevant_standard_view(&mut self, view: StandardView) {
        match self.active_tab {
            Tab::Alignment | Tab::Edit => {
                if self.cameras_linked {
                    let authoritative = if self.workspace.template.is_some() {
                        MeshSide::Template
                    } else {
                        MeshSide::Scan
                    };
                    match authoritative {
                        MeshSide::Scan => {
                            self.workspace.scan_camera.look_from_standard_view(view);
                        }
                        MeshSide::Template => {
                            self.workspace.template_camera.look_from_standard_view(view);
                        }
                    }
                    self.workspace.reconcile_linked_edit_cameras(authoritative);
                } else {
                    self.workspace.scan_camera.look_from_standard_view(view);
                    self.workspace.template_camera.look_from_standard_view(view);
                }
            }
            stage => {
                if let Some(camera) = self.workspace.stage_camera_mut(stage) {
                    camera.look_from_standard_view(view);
                }
            }
        }
    }

    pub(super) fn set_relevant_fov(&mut self, value: f32) {
        match self.active_tab {
            Tab::Alignment | Tab::Edit => {
                if self.cameras_linked {
                    let authoritative = if self.workspace.template.is_some() {
                        MeshSide::Template
                    } else {
                        MeshSide::Scan
                    };
                    match authoritative {
                        MeshSide::Scan => {
                            self.workspace
                                .scan_camera
                                .set_fov_y_degrees_with_dolly_compensation(value);
                        }
                        MeshSide::Template => {
                            self.workspace
                                .template_camera
                                .set_fov_y_degrees_with_dolly_compensation(value);
                        }
                    }
                    self.workspace.reconcile_linked_edit_cameras(authoritative);
                } else {
                    self.workspace
                        .scan_camera
                        .set_fov_y_degrees_with_dolly_compensation(value);
                    self.workspace
                        .template_camera
                        .set_fov_y_degrees_with_dolly_compensation(value);
                }
            }
            stage => {
                if let Some(camera) = self.workspace.stage_camera_mut(stage) {
                    camera.set_fov_y_degrees_with_dolly_compensation(value);
                }
            }
        }
    }
}
