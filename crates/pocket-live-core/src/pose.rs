use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HumanoidBone {
    Hips,
    Spine,
    Chest,
    UpperChest,
    Neck,
    Head,
    LeftShoulder,
    LeftUpperArm,
    LeftLowerArm,
    LeftHand,
    RightShoulder,
    RightUpperArm,
    RightLowerArm,
    RightHand,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneRotation {
    pub bone: HumanoidBone,
    pub rotation: Quat,
    pub weight: f32,
}

/// Shortest-arc rotation that aligns `from` with `to`.
///
/// Returns identity for degenerate or non-finite input. The explicit guard is
/// important because a single corrupt joint must not turn a complete skin
/// palette into NaNs.
pub fn rotation_between(from: Vec3, to: Vec3) -> Quat {
    if !from.is_finite() || !to.is_finite() {
        return Quat::IDENTITY;
    }
    let from_len2 = from.length_squared();
    let to_len2 = to.length_squared();
    if from_len2 <= 1e-10 || to_len2 <= 1e-10 {
        return Quat::IDENTITY;
    }
    let rotation = Quat::from_rotation_arc(from / from_len2.sqrt(), to / to_len2.sqrt());
    if rotation.is_finite() {
        rotation.normalize()
    } else {
        Quat::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_cardinal_directions() {
        let rotation = rotation_between(Vec3::X, Vec3::Y);
        let result = rotation * Vec3::X;
        assert!(result.abs_diff_eq(Vec3::Y, 1e-5), "result={result:?}");
    }

    #[test]
    fn handles_opposite_directions() {
        let rotation = rotation_between(Vec3::X, Vec3::NEG_X);
        let result = rotation * Vec3::X;
        assert!(result.abs_diff_eq(Vec3::NEG_X, 1e-5), "result={result:?}");
    }

    #[test]
    fn degenerate_input_is_safe() {
        assert_eq!(rotation_between(Vec3::ZERO, Vec3::Y), Quat::IDENTITY);
        assert_eq!(rotation_between(Vec3::NAN, Vec3::Y), Quat::IDENTITY);
    }
}
