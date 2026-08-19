import AVFoundation
import CoreImage
import CoreMedia
import Darwin
import Foundation
import Vision

final class VisionTracker: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate, @unchecked Sendable {
    private static let inferenceSize = (width: 640, height: 360)
    private let bodyRequest = VNDetectHumanBodyPoseRequest()
    private let body3DRequest = VNDetectHumanBodyPose3DRequest()
    private let handRequest = VNDetectHumanHandPoseRequest()
    private let faceRequest = VNDetectFaceLandmarksRequest()
    private let bodyEnabled: Bool
    private let body3DEnabled: Bool
    private let faceEnabled: Bool
    private let segmentationRequest: VNGeneratePersonSegmentationRequest?
    private let frameShare: FrameShareWriter?
    private let encoder = JSONEncoder.pocket()
    private let outputLock = NSLock()
    private let inferenceContext = CIContext(options: [.cacheIntermediates: false])
    private let inferencePool: CVPixelBufferPool?
    private let workerLock = NSLock()
    private let handQueue = DispatchQueue(
        label: "dev.pocketjs.live.vision.hands",
        qos: .userInteractive
    )
    private let segmentationQueue = DispatchQueue(
        label: "dev.pocketjs.live.vision.segmentation",
        qos: .userInitiated
    )
    private let faceQueue = DispatchQueue(
        label: "dev.pocketjs.live.vision.face",
        qos: .userInteractive
    )
    private var bodyInFlight = false
    private var handInFlight = false
    private var segmentationInFlight = false
    private var faceInFlight = false
    private var latestHands = [HandObservation.empty(.Left), HandObservation.empty(.Right)]
    private var latestFace: FaceObservation?
    private var previousFaceVisionObservation: VNFaceObservation?
    private var faceRuns: UInt64 = 0
    private var sequence: UInt64 = 0
    private var latestPersonMask: CVPixelBuffer?
    private var lastBodyScheduledNs: UInt64 = 0
    private var lastHandScheduledNs: UInt64 = 0
    private var lastSegmentationScheduledNs: UInt64 = 0
    private var lastFaceScheduledNs: UInt64 = 0
    private let minimumFrameIntervalNs: UInt64
    private let handFrameIntervalNs: UInt64 = 1_000_000_000 / 15
    private let faceFrameIntervalNs: UInt64 = 1_000_000_000 / 15
    private let segmentationFrameIntervalNs: UInt64
    private let maxFrames: UInt64?

    init(
        trackingFPS: UInt32,
        maxFrames: UInt64?,
        frameShare: FrameShareWriter?,
        bodyEnabled: Bool,
        body3DEnabled: Bool,
        faceEnabled: Bool,
        segmentationEvery: UInt64
    ) {
        var pool: CVPixelBufferPool?
        CVPixelBufferPoolCreate(
            kCFAllocatorDefault,
            [kCVPixelBufferPoolMinimumBufferCountKey: 4] as CFDictionary,
            [
                kCVPixelBufferWidthKey: Self.inferenceSize.width,
                kCVPixelBufferHeightKey: Self.inferenceSize.height,
                kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_32BGRA,
                kCVPixelBufferIOSurfacePropertiesKey: [:] as CFDictionary,
            ] as CFDictionary,
            &pool
        )
        inferencePool = pool
        minimumFrameIntervalNs = 1_000_000_000 / UInt64(max(1, trackingFPS))
        segmentationFrameIntervalNs = minimumFrameIntervalNs * max(1, segmentationEvery)
        self.maxFrames = maxFrames
        self.frameShare = frameShare
        self.bodyEnabled = bodyEnabled
        self.body3DEnabled = body3DEnabled
        self.faceEnabled = faceEnabled
        if frameShare != nil {
            let request = VNGeneratePersonSegmentationRequest()
            // The Metal compositor feathers and upsamples the matte. Fast
            // mode keeps the independent body worker responsive on a live
            // 1080p feed; balanced mode materially contends with pose Vision.
            request.qualityLevel = .fast
            request.outputPixelFormat = kCVPixelFormatType_OneComponent8
            segmentationRequest = request
        } else {
            segmentationRequest = nil
        }
        super.init()
        handRequest.maximumHandCount = 2
    }

    func captureOutput(
        _ output: AVCaptureOutput,
        didOutput sampleBuffer: CMSampleBuffer,
        from connection: AVCaptureConnection
    ) {
        guard let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        let timestamp = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        let seconds = CMTimeGetSeconds(timestamp)
        guard seconds.isFinite, seconds >= 0 else { return }
        let capturedAtNs = UInt64(seconds * 1_000_000_000)
        if bodyEnabled,
           due(capturedAtNs, after: lastHandScheduledNs, interval: handFrameIntervalNs),
           scheduleHands(pixelBuffer)
        {
            lastHandScheduledNs = capturedAtNs
        }
        if faceEnabled,
           due(capturedAtNs, after: lastFaceScheduledNs, interval: faceFrameIntervalNs),
           scheduleFace(pixelBuffer)
        {
            lastFaceScheduledNs = capturedAtNs
        }
        if (latestMask() == nil
            || due(
                capturedAtNs,
                after: lastSegmentationScheduledNs,
                interval: segmentationFrameIntervalNs
            )), scheduleSegmentation(pixelBuffer)
        {
            lastSegmentationScheduledNs = capturedAtNs
        }
        if due(capturedAtNs, after: lastBodyScheduledNs, interval: minimumFrameIntervalNs),
           scheduleBody(pixelBuffer, capturedAtNs: capturedAtNs)
        {
            lastBodyScheduledNs = capturedAtNs
        }
    }

    private func due(_ now: UInt64, after previous: UInt64, interval: UInt64) -> Bool {
        previous == 0 || now <= previous || now - previous >= interval
    }

    private func scheduleBody(_ pixelBuffer: CVPixelBuffer, capturedAtNs: UInt64) -> Bool {
        workerLock.lock()
        guard !bodyInFlight else {
            workerLock.unlock()
            return false
        }
        bodyInFlight = true
        workerLock.unlock()
        Task.detached(priority: .userInitiated) { [self] in
            defer {
                workerLock.withLock { bodyInFlight = false }
            }
            do {
                let bodySpace: BodyCoordinateSpace
                let body: BodyObservation
                let hands: [HandObservation]
                if bodyEnabled {
                    let detected = try await detectBodyAndHands(pixelBuffer)
                    bodySpace = detected.0
                    body = supplementBodyWrists(detected.1, with: detected.2)
                    hands = detected.2
                } else {
                    bodySpace = .ImageNormalized
                    body = BodyObservation()
                    hands = [HandObservation.empty(.Left), HandObservation.empty(.Right)]
                }
                sequence &+= 1
                if let frameShare {
                    do {
                        try frameShare.write(
                            camera: pixelBuffer,
                            personMask: latestMask(),
                            sequence: sequence,
                            capturedAtNs: capturedAtNs
                        )
                    } catch {
                        FileHandle.standardError.write(Data("frame-share: \(error)\n".utf8))
                    }
                }
                emit(TrackingFrame(
                    schemaVersion: trackingSchemaVersion,
                    sequence: sequence,
                    capturedAtNs: capturedAtNs,
                    imageSize: [
                        UInt32(CVPixelBufferGetWidth(pixelBuffer)),
                        UInt32(CVPixelBufferGetHeight(pixelBuffer)),
                    ],
                    bodySpace: bodySpace,
                    body: body,
                    hands: hands,
                    face: currentFace()
                ))
                if let maxFrames, sequence >= maxFrames { exit(0) }
            } catch {
                FileHandle.standardError.write(Data("vision body: \(error)\n".utf8))
            }
        }
        return true
    }

    private func detectBodyAndHands(
        _ pixelBuffer: CVPixelBuffer
    ) async throws -> (BodyCoordinateSpace, BodyObservation, [HandObservation]) {
        let visionBuffer = body3DEnabled ? pixelBuffer : inferenceBuffer(from: pixelBuffer)
        if #available(macOS 15.0, *) {
            var request = DetectHumanBodyPoseRequest(.revision2)
            request.detectsHands = true
            let holistic = try await request.perform(on: visionBuffer, orientation: .up).first
            let holisticHands = holistic.map(makeHolisticHands) ?? [
                HandObservation.empty(.Left), HandObservation.empty(.Right),
            ]
            let hands = mergeHands(holisticHands, currentHands())
            if body3DEnabled {
                let handler = VNImageRequestHandler(
                    cvPixelBuffer: inferenceBuffer(from: pixelBuffer),
                    orientation: .up
                )
                try handler.perform([body3DRequest])
                if let observation = body3DRequest.results?.first {
                    return (.CameraRelativeMeters, makeBody3D(observation), hands)
                }
            }
            return (
                .ImageNormalized,
                holistic.map(makeHolisticBody) ?? BodyObservation(),
                hands
            )
        }

        let handler = VNImageRequestHandler(cvPixelBuffer: visionBuffer, orientation: .up)
        var requests: [VNRequest] = [bodyRequest]
        if body3DEnabled { requests.append(body3DRequest) }
        try handler.perform(requests)
        if body3DEnabled, let observation = body3DRequest.results?.first {
            return (.CameraRelativeMeters, makeBody3D(observation), currentHands())
        }
        return (.ImageNormalized, makeBody2D(bodyRequest.results?.first), currentHands())
    }

    @available(macOS 15.0, *)
    private func makeHolisticBody(_ observation: HumanBodyPoseObservation) -> BodyObservation {
        var body = BodyObservation()
        func set(_ slot: JointSlot, _ key: HumanBodyPoseObservation.JointName) {
            guard let point = observation.joint(for: key), point.confidence > 0 else { return }
            body.joints[slot.rawValue] = TrackedPoint3(
                position: [Float(point.location.x), Float(point.location.y), 0],
                confidence: point.confidence
            )
        }
        set(.root, .root)
        set(.neck, .neck)
        set(.head, .nose)
        set(.leftShoulder, .leftShoulder)
        set(.leftElbow, .leftElbow)
        set(.leftWrist, .leftWrist)
        set(.rightShoulder, .rightShoulder)
        set(.rightElbow, .rightElbow)
        set(.rightWrist, .rightWrist)
        set(.leftHip, .leftHip)
        set(.leftKnee, .leftKnee)
        set(.leftAnkle, .leftAnkle)
        set(.rightHip, .rightHip)
        set(.rightKnee, .rightKnee)
        set(.rightAnkle, .rightAnkle)
        if let root = body.joints[JointSlot.root.rawValue],
           let neck = body.joints[JointSlot.neck.rawValue]
        {
            body.joints[JointSlot.spine.rawValue] = TrackedPoint3(
                position: zip(root.position, neck.position).map { ($0 + $1) * 0.5 },
                confidence: min(root.confidence, neck.confidence)
            )
        }
        return body
    }

    @available(macOS 15.0, *)
    private func makeHolisticHands(_ observation: HumanBodyPoseObservation) -> [HandObservation] {
        [
            makeHolisticHand(observation.leftHand, handedness: .Left),
            makeHolisticHand(observation.rightHand, handedness: .Right),
        ]
    }

    @available(macOS 15.0, *)
    private func makeHolisticHand(
        _ observation: HumanHandPoseObservation?,
        handedness: Handedness
    ) -> HandObservation {
        guard let observation else { return HandObservation.empty(handedness) }
        var hand = HandObservation.empty(handedness)
        let keys: [HumanHandPoseObservation.JointName] = [
            .wrist,
            .thumbCMC, .thumbMP, .thumbIP, .thumbTip,
            .indexMCP, .indexPIP, .indexDIP, .indexTip,
            .middleMCP, .middlePIP, .middleDIP, .middleTip,
            .ringMCP, .ringPIP, .ringDIP, .ringTip,
            .littleMCP, .littlePIP, .littleDIP, .littleTip,
        ]
        var confidence: Float = 0
        var count: Float = 0
        for (index, key) in keys.enumerated() {
            guard let point = observation.joint(for: key), point.confidence > 0 else { continue }
            hand.points[index] = TrackedPoint3(
                position: [Float(point.location.x), Float(point.location.y), 0],
                confidence: point.confidence
            )
            confidence += point.confidence
            count += 1
        }
        hand.confidence = count > 0 ? confidence / count : 0
        return hand
    }

    private func scheduleHands(_ pixelBuffer: CVPixelBuffer) -> Bool {
        workerLock.lock()
        guard !handInFlight else {
            workerLock.unlock()
            return false
        }
        handInFlight = true
        workerLock.unlock()
        handQueue.async { [self] in
            autoreleasepool {
                let handler = VNImageRequestHandler(cvPixelBuffer: pixelBuffer, orientation: .up)
                var observations: [VNHumanHandPoseObservation] = []
                do {
                    try handler.perform([handRequest])
                    observations = handRequest.results ?? []
                } catch {
                    FileHandle.standardError.write(Data("vision hands: \(error)\n".utf8))
                }
                let hands = makeHands(observations, body2D: nil)
                workerLock.lock()
                latestHands = hands
                handInFlight = false
                workerLock.unlock()
            }
        }
        return true
    }

    private func scheduleSegmentation(_ pixelBuffer: CVPixelBuffer) -> Bool {
        guard let segmentationRequest else { return false }
        workerLock.lock()
        guard !segmentationInFlight else {
            workerLock.unlock()
            return false
        }
        segmentationInFlight = true
        workerLock.unlock()
        segmentationQueue.async { [self] in
            autoreleasepool {
                let handler = VNImageRequestHandler(
                    cvPixelBuffer: inferenceBuffer(from: pixelBuffer),
                    orientation: .up
                )
                var mask: CVPixelBuffer?
                do {
                    try handler.perform([segmentationRequest])
                    mask = segmentationRequest.results?.first?.pixelBuffer
                } catch {
                    FileHandle.standardError.write(Data("vision segmentation: \(error)\n".utf8))
                }
                workerLock.lock()
                if let mask { latestPersonMask = mask }
                segmentationInFlight = false
                workerLock.unlock()
            }
        }
        return true
    }

    private func scheduleFace(_ pixelBuffer: CVPixelBuffer) -> Bool {
        workerLock.lock()
        guard !faceInFlight else {
            workerLock.unlock()
            return false
        }
        faceInFlight = true
        workerLock.unlock()
        faceQueue.async { [self] in
            autoreleasepool {
                let handler = VNImageRequestHandler(
                    cvPixelBuffer: inferenceBuffer(from: pixelBuffer),
                    orientation: .up
                )
                var face: FaceObservation?
                do {
                    faceRuns &+= 1
                    // Reuse the previous face rectangle for cheap landmark-only
                    // updates, but periodically perform a full reacquisition so
                    // a moved or newly entered face is not missed indefinitely.
                    if faceRuns.isMultiple(of: 30) {
                        faceRequest.inputFaceObservations = nil
                    } else {
                        faceRequest.inputFaceObservations = previousFaceVisionObservation.map { [$0] }
                    }
                    try handler.perform([faceRequest])
                    previousFaceVisionObservation = faceRequest.results?.first
                    face = previousFaceVisionObservation.map(makeFace)
                } catch {
                    previousFaceVisionObservation = nil
                    FileHandle.standardError.write(Data("vision face: \(error)\n".utf8))
                }
                workerLock.lock()
                latestFace = smoothFace(face, previous: latestFace)
                faceInFlight = false
                workerLock.unlock()
            }
        }
        return true
    }

    private func makeFace(_ observation: VNFaceObservation) -> FaceObservation {
        let landmarks = observation.landmarks
        let leftBlink = blinkWeight(landmarks?.leftEye)
        let rightBlink = blinkWeight(landmarks?.rightEye)
        let leftLook = eyeLook(pupil: landmarks?.leftPupil, eye: landmarks?.leftEye)
        let rightLook = eyeLook(pupil: landmarks?.rightPupil, eye: landmarks?.rightEye)
        let lookCount: Float = (leftLook == nil ? 0 : 1) + (rightLook == nil ? 0 : 1)
        let look = if lookCount > 0 {
            [
                ((leftLook?.0 ?? 0) + (rightLook?.0 ?? 0)) / lookCount,
                ((leftLook?.1 ?? 0) + (rightLook?.1 ?? 0)) / lookCount,
            ]
        } else {
            [Float(0), Float(0)]
        }
        let mouthRatio = aspectRatio(landmarks?.innerLips)
        let outerRatio = aspectRatio(landmarks?.outerLips)
        let leftBrow = browRaise(eyebrow: landmarks?.leftEyebrow, eye: landmarks?.leftEye)
        let rightBrow = browRaise(eyebrow: landmarks?.rightEyebrow, eye: landmarks?.rightEye)
        return FaceObservation(
            headRotationRadians: [
                observation.pitch?.floatValue ?? 0,
                observation.yaw?.floatValue ?? 0,
                observation.roll?.floatValue ?? 0,
            ],
            eyeBlink: [leftBlink, rightBlink],
            eyeLook: look,
            mouthOpen: ((mouthRatio - 0.08) / 0.32).clamped01,
            smile: outerRatio > 1e-5 ? ((1.0 / outerRatio - 3.0) / 3.0).clamped01 : 0,
            browRaise: ((leftBrow + rightBrow) * 0.5).clamped01,
            confidence: observation.confidence.clamped01
        )
    }

    private func smoothFace(
        _ sample: FaceObservation?,
        previous: FaceObservation?
    ) -> FaceObservation? {
        guard let sample else { return nil }
        guard let previous else { return sample }
        func mix(_ old: Float, _ new: Float, _ response: Float) -> Float {
            old + (new - old) * response
        }
        return FaceObservation(
            headRotationRadians: zip(
                previous.headRotationRadians,
                sample.headRotationRadians
            ).map { mix($0, $1, 0.35) },
            eyeBlink: zip(previous.eyeBlink, sample.eyeBlink).map { mix($0, $1, 0.7) },
            eyeLook: zip(previous.eyeLook, sample.eyeLook).map { mix($0, $1, 0.45) },
            mouthOpen: mix(previous.mouthOpen, sample.mouthOpen, 0.55),
            smile: mix(previous.smile, sample.smile, 0.35),
            browRaise: mix(previous.browRaise, sample.browRaise, 0.35),
            confidence: sample.confidence
        )
    }

    private func aspectRatio(_ region: VNFaceLandmarkRegion2D?) -> Float {
        guard let bounds = landmarkBounds(region), bounds.width > 1e-5 else { return 0 }
        return bounds.height / bounds.width
    }

    private func blinkWeight(_ eye: VNFaceLandmarkRegion2D?) -> Float {
        guard landmarkBounds(eye) != nil else { return 0 }
        return ((0.28 - aspectRatio(eye)) / 0.18).clamped01
    }

    private func browRaise(
        eyebrow: VNFaceLandmarkRegion2D?,
        eye: VNFaceLandmarkRegion2D?
    ) -> Float {
        guard let brow = landmarkBounds(eyebrow), let eye = landmarkBounds(eye) else { return 0 }
        return ((brow.minY - eye.maxY - 0.015) / 0.12).clamped01
    }

    private func eyeLook(
        pupil: VNFaceLandmarkRegion2D?,
        eye: VNFaceLandmarkRegion2D?
    ) -> (Float, Float)? {
        guard let pupil = pupil?.normalizedPoints.first,
              let eye = landmarkBounds(eye),
              eye.width > 1e-5, eye.height > 1e-5
        else { return nil }
        let x = ((Float(pupil.x) - eye.minX) / eye.width * 2 - 1).clampedSigned
        let y = ((Float(pupil.y) - eye.minY) / eye.height * 2 - 1).clampedSigned
        return (x, y)
    }

    private func landmarkBounds(
        _ region: VNFaceLandmarkRegion2D?
    ) -> (minX: Float, maxX: Float, minY: Float, maxY: Float, width: Float, height: Float)? {
        guard let points = region?.normalizedPoints, !points.isEmpty else { return nil }
        let xs = points.map { Float($0.x) }
        let ys = points.map { Float($0.y) }
        let minX = xs.min()!
        let maxX = xs.max()!
        let minY = ys.min()!
        let maxY = ys.max()!
        return (minX, maxX, minY, maxY, maxX - minX, maxY - minY)
    }

    private func inferenceBuffer(from source: CVPixelBuffer) -> CVPixelBuffer {
        guard let inferencePool else { return source }
        var destination: CVPixelBuffer?
        guard CVPixelBufferPoolCreatePixelBuffer(
            kCFAllocatorDefault,
            inferencePool,
            &destination
        ) == kCVReturnSuccess, let destination else {
            return source
        }
        let sourceWidth = CGFloat(CVPixelBufferGetWidth(source))
        let sourceHeight = CGFloat(CVPixelBufferGetHeight(source))
        let scale = CGAffineTransform(
            scaleX: CGFloat(Self.inferenceSize.width) / sourceWidth,
            y: CGFloat(Self.inferenceSize.height) / sourceHeight
        )
        let image = CIImage(cvPixelBuffer: source).transformed(by: scale)
        inferenceContext.render(
            image,
            to: destination,
            bounds: CGRect(
                x: 0,
                y: 0,
                width: Self.inferenceSize.width,
                height: Self.inferenceSize.height
            ),
            colorSpace: CGColorSpaceCreateDeviceRGB()
        )
        return destination
    }

    private func currentHands() -> [HandObservation] {
        workerLock.lock()
        defer { workerLock.unlock() }
        return latestHands
    }

    private func currentFace() -> FaceObservation? {
        workerLock.lock()
        defer { workerLock.unlock() }
        return latestFace
    }

    private func latestMask() -> CVPixelBuffer? {
        workerLock.lock()
        defer { workerLock.unlock() }
        return latestPersonMask
    }

    private func makeBody2D(_ observation: VNHumanBodyPoseObservation?) -> BodyObservation {
        guard let observation else { return BodyObservation() }
        var body = BodyObservation()

        func set(_ slot: JointSlot, _ key: VNHumanBodyPoseObservation.JointName) {
            guard let point = try? observation.recognizedPoint(key), point.confidence > 0 else {
                return
            }
            body.joints[slot.rawValue] = TrackedPoint3(point)
        }

        set(.root, .root)
        set(.neck, .neck)
        set(.head, .nose)
        set(.leftShoulder, .leftShoulder)
        set(.leftElbow, .leftElbow)
        set(.leftWrist, .leftWrist)
        set(.rightShoulder, .rightShoulder)
        set(.rightElbow, .rightElbow)
        set(.rightWrist, .rightWrist)
        set(.leftHip, .leftHip)
        set(.leftKnee, .leftKnee)
        set(.leftAnkle, .leftAnkle)
        set(.rightHip, .rightHip)
        set(.rightKnee, .rightKnee)
        set(.rightAnkle, .rightAnkle)

        if let root = body.joints[JointSlot.root.rawValue],
           let neck = body.joints[JointSlot.neck.rawValue]
        {
            body.joints[JointSlot.spine.rawValue] = TrackedPoint3(
                position: zip(root.position, neck.position).map { ($0 + $1) * 0.5 },
                confidence: min(root.confidence, neck.confidence)
            )
        }
        return body
    }

    private func makeBody3D(_ observation: VNHumanBodyPose3DObservation) -> BodyObservation {
        var body = BodyObservation()

        func set(_ slot: JointSlot, _ key: VNHumanBodyPose3DObservation.JointName) {
            guard let matrix = try? observation.cameraRelativePosition(key) else { return }
            let p = matrix.columns.3
            let position = [p.x, p.y, p.z]
            guard position.allSatisfy(\.isFinite) else { return }
            body.joints[slot.rawValue] = TrackedPoint3(position: position, confidence: 1)
        }

        set(.root, .root)
        set(.spine, .spine)
        set(.neck, .centerShoulder)
        set(.head, .centerHead)
        set(.leftShoulder, .leftShoulder)
        set(.leftElbow, .leftElbow)
        set(.leftWrist, .leftWrist)
        set(.rightShoulder, .rightShoulder)
        set(.rightElbow, .rightElbow)
        set(.rightWrist, .rightWrist)
        set(.leftHip, .leftHip)
        set(.leftKnee, .leftKnee)
        set(.leftAnkle, .leftAnkle)
        set(.rightHip, .rightHip)
        set(.rightKnee, .rightKnee)
        set(.rightAnkle, .rightAnkle)
        return body
    }

    private func makeHands(
        _ observations: [VNHumanHandPoseObservation],
        body2D: VNHumanBodyPoseObservation?
    ) -> [HandObservation] {
        var result = [HandObservation.empty(.Left), HandObservation.empty(.Right)]
        var occupied = [false, false]

        for observation in observations {
            guard let wrist = try? observation.recognizedPoint(.wrist), wrist.confidence > 0 else {
                continue
            }
            let side = nearestSide(to: wrist, body2D: body2D, occupied: occupied)
            let resultIndex = side == .Left ? 0 : 1
            guard !occupied[resultIndex] else { continue }
            occupied[resultIndex] = true

            var hand = HandObservation.empty(side)
            let keys: [VNHumanHandPoseObservation.JointName] = [
                .wrist,
                .thumbCMC, .thumbMP, .thumbIP, .thumbTip,
                .indexMCP, .indexPIP, .indexDIP, .indexTip,
                .middleMCP, .middlePIP, .middleDIP, .middleTip,
                .ringMCP, .ringPIP, .ringDIP, .ringTip,
                .littleMCP, .littlePIP, .littleDIP, .littleTip,
            ]
            var confidenceSum: Float = 0
            var confidenceCount: Float = 0
            for (index, key) in keys.enumerated() {
                guard let point = try? observation.recognizedPoint(key), point.confidence > 0 else {
                    continue
                }
                hand.points[index] = TrackedPoint3(point)
                confidenceSum += point.confidence
                confidenceCount += 1
            }
            hand.confidence = confidenceCount > 0 ? confidenceSum / confidenceCount : 0
            result[resultIndex] = hand
        }
        return result
    }

    private func nearestSide(
        to wrist: VNRecognizedPoint,
        body2D: VNHumanBodyPoseObservation?,
        occupied: [Bool]
    ) -> Handedness {
        func distanceSquared(_ key: VNHumanBodyPoseObservation.JointName) -> Float? {
            guard let point = try? body2D?.recognizedPoint(key), point.confidence > 0 else {
                return nil
            }
            let dx = Float(wrist.location.x - point.location.x)
            let dy = Float(wrist.location.y - point.location.y)
            return dx * dx + dy * dy
        }

        let leftDistance = occupied[0] ? nil : distanceSquared(.leftWrist)
        let rightDistance = occupied[1] ? nil : distanceSquared(.rightWrist)
        switch (leftDistance, rightDistance) {
        case let (left?, right?): return left <= right ? .Left : .Right
        case (_?, nil): return .Left
        case (nil, _?): return .Right
        case (nil, nil):
            // Subject-left normally appears on image-right for an unmirrored
            // front-facing camera. Body wrists are preferred whenever present.
            return wrist.location.x >= 0.5 ? .Left : .Right
        }
    }

    private func mergeHands(
        _ holistic: [HandObservation],
        _ dedicated: [HandObservation]
    ) -> [HandObservation] {
        [0, 1].map { index in
            dedicated[index].confidence >= holistic[index].confidence
                ? dedicated[index]
                : holistic[index]
        }
    }

    private func supplementBodyWrists(
        _ detected: BodyObservation,
        with hands: [HandObservation]
    ) -> BodyObservation {
        var body = detected
        let mappings: [(hand: Int, joint: JointSlot)] = [
            (0, .leftWrist),
            (1, .rightWrist),
        ]
        for mapping in mappings {
            guard body.joints[mapping.joint.rawValue] == nil,
                  hands[mapping.hand].confidence >= 0.3,
                  let wrist = hands[mapping.hand].points[0],
                  wrist.confidence >= 0.3
            else { continue }
            body.joints[mapping.joint.rawValue] = wrist
        }
        return body
    }

    private func emit(_ frame: TrackingFrame) {
        guard var data = try? encoder.encode(frame) else { return }
        data.append(0x0A)
        outputLock.lock()
        defer { outputLock.unlock() }
        FileHandle.standardOutput.write(data)
    }
}

private extension Float {
    var clamped01: Float { min(max(self, 0), 1) }
    var clampedSigned: Float { min(max(self, -1), 1) }
}
