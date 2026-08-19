#!/usr/bin/env python3
"""Local camera -> MediaPipe face/pose/hand controls -> NDJSON.

The helper deliberately exposes semantic blendshapes and the small set of
skeletal points Pocket consumes rather than leaking MediaPipe-specific result
objects into the host. It performs no networking and never persists frames.
"""

from __future__ import annotations

import argparse
from contextlib import ExitStack
import ctypes
import json
import math
import mmap
import os
import struct
import sys
import time
from pathlib import Path

import cv2
import mediapipe as mp
import numpy as np


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--pose-model", type=Path, required=True)
    parser.add_argument("--hand-model", type=Path, required=True)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--camera-index", type=int)
    source.add_argument("--frame-share-name")
    parser.add_argument("--max-frames", type=int)
    parser.add_argument("--fps", type=float, default=15.0)
    return parser.parse_args()


class SharedFrameSource:
    HEADER_BYTES = 64
    CAPACITY = HEADER_BYTES + 1920 * 1080 * 5

    def __init__(self, name: str) -> None:
        libc = ctypes.CDLL(None, use_errno=True)
        libc.shm_open.argtypes = [ctypes.c_char_p, ctypes.c_int, ctypes.c_uint]
        libc.shm_open.restype = ctypes.c_int
        fd = libc.shm_open(name.encode(), os.O_RDONLY, 0)
        if fd < 0:
            errno = ctypes.get_errno()
            raise OSError(errno, os.strerror(errno), name)
        try:
            self.mapping = mmap.mmap(fd, self.CAPACITY, access=mmap.ACCESS_READ)
        finally:
            os.close(fd)
        self.last_sequence = 0

    def read(self) -> tuple[int, object] | None:
        sequence = struct.unpack_from("<Q", self.mapping, 16)[0]
        if sequence == 0 or sequence <= self.last_sequence:
            return None
        header = self.mapping[: self.HEADER_BYTES]
        if header[:8] != b"PKLVFRM\0":
            return None
        (
            version,
            header_bytes,
            published,
            captured_at_ns,
            width,
            height,
            stride,
            bgra_bytes,
        ) = struct.unpack_from("<IIQQIIII", header, 8)
        if (
            version != 1
            or header_bytes != self.HEADER_BYTES
            or published != sequence
            or width == 0
            or height == 0
            or width > 1920
            or height > 1080
            or stride != width * 4
            or bgra_bytes != width * height * 4
        ):
            return None
        raw = self.mapping[self.HEADER_BYTES : self.HEADER_BYTES + bgra_bytes]
        if struct.unpack_from("<Q", self.mapping, 16)[0] != sequence:
            return None
        self.last_sequence = sequence
        bgra = np.frombuffer(raw, dtype="uint8").reshape((height, width, 4)).copy()
        inference = cv2.resize(bgra, (640, 360), interpolation=cv2.INTER_AREA)
        rgb = cv2.cvtColor(inference, cv2.COLOR_BGRA2RGB)
        return captured_at_ns, mp.Image(image_format=mp.ImageFormat.SRGB, data=rgb)

    def close(self) -> None:
        self.mapping.close()


def category_scores(result: object) -> dict[str, float]:
    if not result.face_blendshapes:
        return {}
    classifications = result.face_blendshapes[0]
    categories = getattr(classifications, "categories", classifications)
    return {
        category.category_name: float(category.score)
        for category in categories
        if category.category_name
    }


def head_euler(result: object) -> list[float]:
    if not result.facial_transformation_matrixes:
        return [0.0, 0.0, 0.0]
    matrix = result.facial_transformation_matrixes[0]
    # Canonical-face -> observed-face rotation. These formulae decompose the
    # upper-left 3x3 into pitch/yaw/roll and avoid an optional scipy runtime.
    pitch = math.asin(max(-1.0, min(1.0, -float(matrix[1][2]))))
    yaw = math.atan2(float(matrix[0][2]), float(matrix[2][2]))
    roll = math.atan2(float(matrix[1][0]), float(matrix[1][1]))
    return [pitch, yaw, roll]


def score(scores: dict[str, float], name: str) -> float:
    return max(0.0, min(1.0, scores.get(name, 0.0)))


def face_observation(result: object) -> dict[str, object] | None:
    if not result.face_landmarks:
        return None
    scores = category_scores(result)
    left_look_x = score(scores, "eyeLookOutLeft") - score(scores, "eyeLookInLeft")
    right_look_x = score(scores, "eyeLookInRight") - score(scores, "eyeLookOutRight")
    look_y = (
        score(scores, "eyeLookUpLeft")
        + score(scores, "eyeLookUpRight")
        - score(scores, "eyeLookDownLeft")
        - score(scores, "eyeLookDownRight")
    ) * 0.5
    return {
        "head_rotation_radians": head_euler(result),
        "eye_blink": [
            score(scores, "eyeBlinkLeft"),
            score(scores, "eyeBlinkRight"),
        ],
        "eye_look": [
            max(-1.0, min(1.0, (left_look_x + right_look_x) * 0.5)),
            max(-1.0, min(1.0, look_y)),
        ],
        "mouth_open": score(scores, "jawOpen"),
        "smile": (
            score(scores, "mouthSmileLeft") + score(scores, "mouthSmileRight")
        ) * 0.5,
        "brow_raise": max(
            score(scores, "browInnerUp"),
            (score(scores, "browOuterUpLeft") + score(scores, "browOuterUpRight"))
            * 0.5,
        ),
        "confidence": 1.0,
    }


def clamp01(value: float) -> float:
    return max(0.0, min(1.0, float(value)))


def landmark_confidence(landmark: object, fallback: float = 1.0) -> float:
    values = []
    for name in ("visibility", "presence"):
        value = getattr(landmark, name, None)
        if value is not None and math.isfinite(float(value)):
            values.append(float(value))
    return clamp01(min(values) if values else fallback)


def tracked_point(landmark: object, confidence: float | None = None) -> dict[str, object]:
    return {
        # MediaPipe images use an upper-left origin. Pocket image-space uses
        # lower-left so that positive Y agrees with the VRM coordinate system.
        "position": [float(landmark.x), 1.0 - float(landmark.y), 0.0],
        "confidence": clamp01(
            landmark_confidence(landmark) if confidence is None else confidence
        ),
    }


def midpoint(left: object, right: object) -> dict[str, object]:
    return {
        "position": [
            (float(left.x) + float(right.x)) * 0.5,
            1.0 - (float(left.y) + float(right.y)) * 0.5,
            0.0,
        ],
        "confidence": min(landmark_confidence(left), landmark_confidence(right)),
    }


def midpoint_points(left: dict[str, object], right: dict[str, object]) -> dict[str, object]:
    return {
        "position": [
            (float(left["position"][axis]) + float(right["position"][axis])) * 0.5
            for axis in range(3)
        ],
        "confidence": min(float(left["confidence"]), float(right["confidence"])),
    }


def body_observation(result: object) -> dict[str, object] | None:
    if not result.pose_landmarks:
        return None
    landmarks = result.pose_landmarks[0]
    joints: list[dict[str, object] | None] = [None] * 16
    # Pocket BodyJoint order. Pose Landmarker indexes are the public 33-point
    # BlazePose contract.
    root = midpoint(landmarks[23], landmarks[24])
    neck = midpoint(landmarks[11], landmarks[12])
    joints[0] = root
    joints[1] = midpoint_points(root, neck)
    joints[2] = neck
    joints[3] = tracked_point(landmarks[0])
    for pocket_index, pose_index in (
        (4, 11), (5, 13), (6, 15),
        (7, 12), (8, 14), (9, 16),
        (10, 23), (11, 25), (12, 27),
        (13, 24), (14, 26), (15, 28),
    ):
        joints[pocket_index] = tracked_point(landmarks[pose_index])
    return {"joints": joints}


def hand_category(entry: object) -> tuple[str | None, float]:
    categories = getattr(entry, "categories", entry)
    if not categories:
        return None, 0.0
    category = categories[0]
    return getattr(category, "category_name", None), clamp01(category.score)


def hand_observations(
    hand_result: object, pose_result: object
) -> list[dict[str, object]]:
    output = [
        {"handedness": "Left", "points": [None] * 21, "confidence": 0.0},
        {"handedness": "Right", "points": [None] * 21, "confidence": 0.0},
    ]
    pose_wrists = None
    if pose_result.pose_landmarks:
        pose = pose_result.pose_landmarks[0]
        pose_wrists = [pose[15], pose[16]]

    candidates: list[tuple[int, float, list[object]]] = []
    for index, landmarks in enumerate(hand_result.hand_landmarks):
        label, category_score = hand_category(hand_result.handedness[index])
        if pose_wrists is not None:
            wrist = landmarks[0]
            distances = [
                (float(wrist.x) - float(target.x)) ** 2
                + (float(wrist.y) - float(target.y)) ** 2
                for target in pose_wrists
            ]
            side = 0 if distances[0] <= distances[1] else 1
        else:
            side = 0 if label == "Left" else 1
        candidates.append((side, category_score, landmarks))

    # If two detections compete for one side, keep the more confident one.
    for side, category_score, landmarks in sorted(
        candidates, key=lambda candidate: candidate[1]
    ):
        output[side] = {
            "handedness": "Left" if side == 0 else "Right",
            "points": [tracked_point(point, category_score) for point in landmarks],
            "confidence": category_score,
        }
    return output


def supplement_body_wrists(
    body: dict[str, object] | None, hands: list[dict[str, object]]
) -> dict[str, object] | None:
    if body is None:
        return None
    joints = body["joints"]
    for hand_index, shoulder_index, elbow_index, wrist_index in (
        (0, 4, 5, 6),
        (1, 7, 8, 9),
    ):
        wrist = hands[hand_index]["points"][0]
        current = joints[wrist_index]
        if wrist is not None and (
            current is None
            or float(wrist["confidence"]) > float(current["confidence"])
        ):
            joints[wrist_index] = wrist
        # BlazePose still predicts a useful elbow position when a forearm is
        # partly occluded, but its visibility score can collapse. A separately
        # detected hand supplies strong endpoint evidence, so retain that elbow
        # with a conservative derived confidence instead of dropping the whole
        # chain.
        shoulder = joints[shoulder_index]
        elbow = joints[elbow_index]
        wrist = joints[wrist_index]
        if shoulder is not None and elbow is not None and wrist is not None:
            endpoint_confidence = min(
                float(shoulder["confidence"]), float(wrist["confidence"])
            )
            if endpoint_confidence >= 0.4:
                elbow["confidence"] = max(
                    float(elbow["confidence"]), endpoint_confidence * 0.65
                )
    return body


def main() -> int:
    args = parse_args()
    for label, path in (
        ("face", args.model),
        ("pose", args.pose_model),
        ("hand", args.hand_model),
    ):
        if not path.is_file():
            print(f"{label} model not found: {path}", file=sys.stderr)
            return 2

    capture = None
    shared = None
    if args.frame_share_name:
        shared = SharedFrameSource(args.frame_share_name)
    else:
        capture = cv2.VideoCapture(args.camera_index, cv2.CAP_AVFOUNDATION)
        capture.set(cv2.CAP_PROP_FRAME_WIDTH, 640)
        capture.set(cv2.CAP_PROP_FRAME_HEIGHT, 360)
        capture.set(cv2.CAP_PROP_FPS, 30)
        if not capture.isOpened():
            print(f"cannot open camera index {args.camera_index}", file=sys.stderr)
            return 3

    face_options = mp.tasks.vision.FaceLandmarkerOptions(
        base_options=mp.tasks.BaseOptions(
            model_asset_path=str(args.model),
            delegate=mp.tasks.BaseOptions.Delegate.CPU,
        ),
        running_mode=mp.tasks.vision.RunningMode.VIDEO,
        num_faces=1,
        min_face_detection_confidence=0.5,
        min_face_presence_confidence=0.5,
        min_tracking_confidence=0.5,
        output_face_blendshapes=True,
        output_facial_transformation_matrixes=True,
    )
    pose_options = mp.tasks.vision.PoseLandmarkerOptions(
        base_options=mp.tasks.BaseOptions(
            model_asset_path=str(args.pose_model),
            delegate=mp.tasks.BaseOptions.Delegate.CPU,
        ),
        running_mode=mp.tasks.vision.RunningMode.VIDEO,
        num_poses=1,
        min_pose_detection_confidence=0.45,
        min_pose_presence_confidence=0.45,
        min_tracking_confidence=0.5,
        output_segmentation_masks=False,
    )
    hand_options = mp.tasks.vision.HandLandmarkerOptions(
        base_options=mp.tasks.BaseOptions(
            model_asset_path=str(args.hand_model),
            delegate=mp.tasks.BaseOptions.Delegate.CPU,
        ),
        running_mode=mp.tasks.vision.RunningMode.VIDEO,
        num_hands=2,
        min_hand_detection_confidence=0.4,
        min_hand_presence_confidence=0.4,
        min_tracking_confidence=0.5,
    )
    interval = 1.0 / max(1.0, args.fps)
    emitted = 0
    started = time.monotonic()
    next_frame = started
    try:
        with ExitStack() as stack:
            face_landmarker = stack.enter_context(
                mp.tasks.vision.FaceLandmarker.create_from_options(face_options)
            )
            pose_landmarker = stack.enter_context(
                mp.tasks.vision.PoseLandmarker.create_from_options(pose_options)
            )
            hand_landmarker = stack.enter_context(
                mp.tasks.vision.HandLandmarker.create_from_options(hand_options)
            )
            while args.max_frames is None or emitted < args.max_frames:
                now = time.monotonic()
                if now < next_frame:
                    time.sleep(min(0.005, next_frame - now))
                    continue
                next_frame = max(next_frame + interval, now)
                if shared:
                    shared_frame = shared.read()
                    if shared_frame is None:
                        time.sleep(0.002)
                        continue
                    captured_at_ns, image = shared_frame
                else:
                    ok, frame = capture.read()
                    if not ok:
                        continue
                    captured_at_ns = time.monotonic_ns()
                    rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
                    image = mp.Image(image_format=mp.ImageFormat.SRGB, data=rgb)
                timestamp_ms = int((now - started) * 1000)
                face_result = face_landmarker.detect_for_video(image, timestamp_ms)
                pose_result = pose_landmarker.detect_for_video(image, timestamp_ms)
                hand_result = hand_landmarker.detect_for_video(image, timestamp_ms)
                hands = hand_observations(hand_result, pose_result)
                body = supplement_body_wrists(body_observation(pose_result), hands)
                payload = {
                    "captured_at_ns": captured_at_ns,
                    "face": face_observation(face_result),
                    "body": body,
                    "hands": hands,
                }
                print(json.dumps(payload, separators=(",", ":")), flush=True)
                emitted += 1
    finally:
        if capture:
            capture.release()
        if shared:
            shared.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
