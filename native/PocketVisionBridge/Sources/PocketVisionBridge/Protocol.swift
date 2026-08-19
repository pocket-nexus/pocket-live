import Foundation
import Vision

let trackingSchemaVersion: UInt16 = 3

enum BodyCoordinateSpace: String, Codable {
    case ImageNormalized
    case CameraRelativeMeters
}

struct TrackedPoint3: Codable {
    let position: [Float]
    let confidence: Float

    init(_ point: VNRecognizedPoint) {
        position = [Float(point.location.x), Float(point.location.y), 0]
        confidence = point.confidence
    }

    init(position: [Float], confidence: Float) {
        self.position = position
        self.confidence = confidence
    }
}

struct BodyObservation: Codable {
    /// Must stay index-aligned with pocket_live_core::BodyJoint.
    var joints: [TrackedPoint3?] = Array(repeating: nil, count: 16)
}

enum Handedness: String, Codable {
    case Left
    case Right
}

struct HandObservation: Codable {
    let handedness: Handedness
    var points: [TrackedPoint3?] = Array(repeating: nil, count: 21)
    var confidence: Float = 0

    static func empty(_ handedness: Handedness) -> Self {
        Self(handedness: handedness)
    }
}

struct FaceObservation: Codable {
    let headRotationRadians: [Float]
    let eyeBlink: [Float]
    let eyeLook: [Float]
    let mouthOpen: Float
    let smile: Float
    let browRaise: Float
    let confidence: Float

    enum CodingKeys: String, CodingKey {
        case headRotationRadians = "head_rotation_radians"
        case eyeBlink = "eye_blink"
        case eyeLook = "eye_look"
        case mouthOpen = "mouth_open"
        case smile
        case browRaise = "brow_raise"
        case confidence
    }
}

struct TrackingFrame: Codable {
    let schemaVersion: UInt16
    let sequence: UInt64
    let capturedAtNs: UInt64
    let imageSize: [UInt32]
    let bodySpace: BodyCoordinateSpace
    let body: BodyObservation
    let hands: [HandObservation]
    let face: FaceObservation?

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case sequence
        case capturedAtNs = "captured_at_ns"
        case imageSize = "image_size"
        case bodySpace = "body_space"
        case body
        case hands
        case face
    }
}

struct CameraDeviceInfo: Codable {
    let id: String
    let name: String
    let suspended: Bool
    let max1080pFPS: Int

    enum CodingKeys: String, CodingKey {
        case id, name, suspended
        case max1080pFPS = "max_1080p_fps"
    }
}

enum JointSlot: Int {
    case root = 0
    case spine
    case neck
    case head
    case leftShoulder
    case leftElbow
    case leftWrist
    case rightShoulder
    case rightElbow
    case rightWrist
    case leftHip
    case leftKnee
    case leftAnkle
    case rightHip
    case rightKnee
    case rightAnkle
}

extension JSONEncoder {
    static func pocket() -> JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return encoder
    }
}
