/// Returns the canonical workspace marker used by smoke tests.
#[must_use]
pub fn workspace_marker() -> &'static str {
    "codeatlas-workspace-ready"
}

#[cfg(test)]
mod tests {
    use super::workspace_marker;

    #[test]
    fn workspace_marker_is_stable() {
        assert_eq!(workspace_marker(), "codeatlas-workspace-ready");
    }
}
