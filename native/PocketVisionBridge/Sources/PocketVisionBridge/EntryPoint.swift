import AVFoundation
import CoreVideo
import Darwin
import Foundation

@main
enum PocketVisionBridgeMain {
    static func main() async {
        let args = Array(CommandLine.arguments.dropFirst())
        if args.contains("--help") {
            printHelp()
            return
        }
        if args.contains("--mock") {
            emitMockFrame(frameShareName: value(after: "--frame-share-name", in: args))
            return
        }
        if args.contains("--list-devices") {
            listDevices()
            return
        }

        guard await requestCameraAccess() else {
            fail("camera permission denied; enable it in System Settings > Privacy & Security > Camera")
        }
        do {
            let deviceID = value(after: "--device", in: args)
            let trackingFPS = UInt32(value(after: "--tracking-fps", in: args) ?? "30") ?? 30
            let maxFrames = value(after: "--max-frames", in: args).flatMap(UInt64.init)
            let frameShareName = value(after: "--frame-share-name", in: args)
            let bodyMode = value(after: "--body-mode", in: args) ?? "2d"
            guard bodyMode == "none" || bodyMode == "2d" || bodyMode == "3d" else {
                fail("--body-mode must be none, 2d, or 3d")
            }
            let faceMode = value(after: "--face-mode", in: args) ?? "landmarks"
            guard faceMode == "landmarks" || faceMode == "none" else {
                fail("--face-mode must be landmarks or none")
            }
            let segmentationEvery = UInt64(value(after: "--segmentation-every", in: args) ?? "3") ?? 3
            let runtime = try makeCaptureSession(
                deviceID: deviceID,
                trackingFPS: trackingFPS,
                maxFrames: maxFrames,
                frameShareName: frameShareName,
                bodyEnabled: bodyMode != "none",
                body3DEnabled: bodyMode == "3d",
                faceEnabled: faceMode == "landmarks",
                segmentationEvery: max(1, segmentationEvery)
            )
            FileHandle.standardError.write(
                Data(
                    "ready: tracking_fps=\(trackingFPS), inference=640x360, body=\(bodyMode), face=\(faceMode), segmentation_every=\(segmentationEvery), schema=\(trackingSchemaVersion)\n".utf8
                )
            )
            runtime.session.startRunning()
            withExtendedLifetime(runtime.tracker) {
                RunLoop.current.run()
            }
        } catch {
            fail("\(error)")
        }
    }

    private static func makeCaptureSession(
        deviceID: String?,
        trackingFPS: UInt32,
        maxFrames: UInt64?,
        frameShareName: String?,
        bodyEnabled: Bool,
        body3DEnabled: Bool,
        faceEnabled: Bool,
        segmentationEvery: UInt64
    ) throws -> CaptureRuntime {
        let session = AVCaptureSession()
        session.beginConfiguration()
        defer { session.commitConfiguration() }
        session.sessionPreset = .hd1920x1080

        let devices = allDevices()
        let device: AVCaptureDevice?
        if let deviceID {
            device = devices.first(where: { $0.uniqueID == deviceID })
        } else {
            let preferred = AVCaptureDevice.default(for: .video)
            device = if let preferred, !preferred.isSuspended {
                preferred
            } else {
                devices.first(where: { !$0.isSuspended })
            }
        }
        guard let device else { throw BridgeError("no video camera found") }
        guard !device.isSuspended else {
            throw BridgeError(
                "camera '\(device.localizedName)' is suspended; open the MacBook lid or select another --device"
            )
        }
        let captureMode = try configureCapture(device: device, preferredFPS: 60)
        let input = try AVCaptureDeviceInput(device: device)
        guard session.canAddInput(input) else { throw BridgeError("cannot add camera input") }
        session.addInput(input)

        let output = AVCaptureVideoDataOutput()
        output.alwaysDiscardsLateVideoFrames = true
        let frameShare = try frameShareName.map(FrameShareWriter.init(name:))
        output.videoSettings = [
            kCVPixelBufferPixelFormatTypeKey as String:
                frameShare == nil
                ? kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
                : kCVPixelFormatType_32BGRA,
        ]
        let tracker = VisionTracker(
            trackingFPS: trackingFPS,
            maxFrames: maxFrames,
            frameShare: frameShare,
            bodyEnabled: bodyEnabled,
            body3DEnabled: body3DEnabled,
            faceEnabled: faceEnabled,
            segmentationEvery: segmentationEvery
        )
        // AVCaptureVideoDataOutput retains its sample-buffer delegate.
        output.setSampleBufferDelegate(
            tracker,
            queue: DispatchQueue(label: "dev.pocketjs.live.vision", qos: .userInteractive)
        )
        guard session.canAddOutput(output) else { throw BridgeError("cannot add camera output") }
        session.addOutput(output)

        FileHandle.standardError.write(
            Data(
                "camera: \(device.localizedName), capture=\(captureMode.width)x\(captureMode.height)@\(captureMode.fps)\n".utf8
            )
        )
        return CaptureRuntime(session: session, tracker: tracker)
    }

    private static func configureCapture(
        device: AVCaptureDevice,
        preferredFPS: Double
    ) throws -> (width: Int32, height: Int32, fps: Int) {
        struct Candidate {
            let format: AVCaptureDevice.Format
            let width: Int32
            let height: Int32
            let fps: Double
        }
        let candidates = device.formats.compactMap { format -> Candidate? in
            let dimensions = CMVideoFormatDescriptionGetDimensions(format.formatDescription)
            guard dimensions.width == 1920, dimensions.height == 1080 else { return nil }
            let supported = format.videoSupportedFrameRateRanges
                .map(\.maxFrameRate)
                .filter { $0 >= 24 }
                .max()
            guard let supported else { return nil }
            return Candidate(
                format: format,
                width: dimensions.width,
                height: dimensions.height,
                fps: min(preferredFPS, supported)
            )
        }
        guard let selected = candidates.max(by: { left, right in
            let leftPreferred = left.fps >= preferredFPS
            let rightPreferred = right.fps >= preferredFPS
            return leftPreferred == rightPreferred ? left.fps < right.fps : !leftPreferred
        }) else {
            // Let the 1080p session preset pick the best device-native rate.
            let dimensions = CMVideoFormatDescriptionGetDimensions(device.activeFormat.formatDescription)
            return (dimensions.width, dimensions.height, 0)
        }

        try device.lockForConfiguration()
        defer { device.unlockForConfiguration() }
        device.activeFormat = selected.format
        let duration = CMTime(value: 1, timescale: CMTimeScale(selected.fps.rounded()))
        device.activeVideoMinFrameDuration = duration
        device.activeVideoMaxFrameDuration = duration
        return (selected.width, selected.height, Int(selected.fps.rounded()))
    }

    private static func requestCameraAccess() async -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized: return true
        case .notDetermined: return await AVCaptureDevice.requestAccess(for: .video)
        default: return false
        }
    }

    private static func allDevices() -> [AVCaptureDevice] {
        AVCaptureDevice.DiscoverySession(
            deviceTypes: [.builtInWideAngleCamera, .continuityCamera, .external],
            mediaType: .video,
            position: .unspecified
        ).devices
    }

    private static func listDevices() {
        let devices = allDevices().map {
            CameraDeviceInfo(
                id: $0.uniqueID,
                name: $0.localizedName,
                suspended: $0.isSuspended,
                max1080pFPS: maximum1080pFPS($0)
            )
        }
        let encoder = JSONEncoder.pocket()
        guard let data = try? encoder.encode(devices) else { fail("cannot encode device list") }
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data([0x0A]))
    }

    private static func maximum1080pFPS(_ device: AVCaptureDevice) -> Int {
        Int(device.formats.compactMap { format -> Double? in
            let dimensions = CMVideoFormatDescriptionGetDimensions(format.formatDescription)
            guard dimensions.width == 1920, dimensions.height == 1080 else { return nil }
            return format.videoSupportedFrameRateRanges.map(\.maxFrameRate).max()
        }.max()?.rounded() ?? 0)
    }

    private static func emitMockFrame(frameShareName: String?) {
        var body = BodyObservation()
        body.joints[JointSlot.root.rawValue] = TrackedPoint3(
            position: [0.5, 0.35, 0], confidence: 0.99
        )
        body.joints[JointSlot.spine.rawValue] = TrackedPoint3(
            position: [0.5, 0.53, 0], confidence: 0.99
        )
        body.joints[JointSlot.neck.rawValue] = TrackedPoint3(
            position: [0.5, 0.72, 0], confidence: 0.98
        )
        body.joints[JointSlot.head.rawValue] = TrackedPoint3(
            position: [0.5, 0.86, 0], confidence: 0.97
        )
        body.joints[JointSlot.leftShoulder.rawValue] = TrackedPoint3(
            position: [0.62, 0.66, 0], confidence: 0.97
        )
        body.joints[JointSlot.leftElbow.rawValue] = TrackedPoint3(
            position: [0.73, 0.56, 0], confidence: 0.96
        )
        body.joints[JointSlot.leftWrist.rawValue] = TrackedPoint3(
            position: [0.82, 0.69, 0], confidence: 0.95
        )
        body.joints[JointSlot.rightShoulder.rawValue] = TrackedPoint3(
            position: [0.38, 0.66, 0], confidence: 0.97
        )
        body.joints[JointSlot.rightElbow.rawValue] = TrackedPoint3(
            position: [0.27, 0.56, 0], confidence: 0.96
        )
        body.joints[JointSlot.rightWrist.rawValue] = TrackedPoint3(
            position: [0.18, 0.69, 0], confidence: 0.95
        )
        let frame = TrackingFrame(
            schemaVersion: trackingSchemaVersion,
            sequence: 1,
            capturedAtNs: 1_000_000_000,
            imageSize: [1920, 1080],
            bodySpace: .ImageNormalized,
            body: body,
            hands: [HandObservation.empty(.Left), HandObservation.empty(.Right)],
            face: FaceObservation(
                headRotationRadians: [0.05, -0.1, 0.02],
                eyeBlink: [0.15, 0.2],
                eyeLook: [0.25, -0.1],
                mouthOpen: 0.35,
                smile: 0.1,
                browRaise: 0.2,
                confidence: 0.95
            )
        )
        if let frameShareName {
            do {
                let writer = try FrameShareWriter(name: frameShareName)
                let camera = try mockPixelBuffer(width: 64, height: 36, mask: false)
                let mask = try mockPixelBuffer(width: 64, height: 36, mask: true)
                try writer.write(
                    camera: camera,
                    personMask: mask,
                    sequence: frame.sequence,
                    capturedAtNs: frame.capturedAtNs
                )
            } catch {
                fail("mock frame share: \(error)")
            }
        }
        let encoder = JSONEncoder.pocket()
        guard var data = try? encoder.encode(frame) else { fail("cannot encode mock frame") }
        data.append(0x0A)
        FileHandle.standardOutput.write(data)
    }

    private static func mockPixelBuffer(
        width: Int,
        height: Int,
        mask: Bool
    ) throws -> CVPixelBuffer {
        var pixelBuffer: CVPixelBuffer?
        let format = mask ? kCVPixelFormatType_OneComponent8 : kCVPixelFormatType_32BGRA
        let status = CVPixelBufferCreate(
            kCFAllocatorDefault,
            width,
            height,
            format,
            nil,
            &pixelBuffer
        )
        guard status == kCVReturnSuccess, let pixelBuffer else {
            throw BridgeError("creating mock pixel buffer: \(status)")
        }
        CVPixelBufferLockBaseAddress(pixelBuffer, [])
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, []) }
        guard let base = CVPixelBufferGetBaseAddress(pixelBuffer) else {
            throw BridgeError("mock pixel buffer has no base address")
        }
        let stride = CVPixelBufferGetBytesPerRow(pixelBuffer)
        for y in 0..<height {
            for x in 0..<width {
                if mask {
                    let dx = Float(x) / Float(width) - 0.5
                    let dy = Float(y) / Float(height) - 0.5
                    base.storeBytes(
                        of: dx * dx + dy * dy < 0.08 ? UInt8(255) : UInt8(0),
                        toByteOffset: y * stride + x,
                        as: UInt8.self
                    )
                } else {
                    let offset = y * stride + x * 4
                    base.storeBytes(of: UInt8(70 + y * 3), toByteOffset: offset, as: UInt8.self)
                    base.storeBytes(of: UInt8(35 + x * 2), toByteOffset: offset + 1, as: UInt8.self)
                    base.storeBytes(of: UInt8(24), toByteOffset: offset + 2, as: UInt8.self)
                    base.storeBytes(of: UInt8(255), toByteOffset: offset + 3, as: UInt8.self)
                }
            }
        }
        return pixelBuffer
    }

    private static func value(after flag: String, in args: [String]) -> String? {
        guard let index = args.firstIndex(of: flag), args.indices.contains(index + 1) else {
            return nil
        }
        return args[index + 1]
    }

    private static func printHelp() {
        print("""
        pocket-vision-bridge

          --list-devices              print local camera devices as JSON
          --device ID                 select a camera unique ID
          --tracking-fps N            throttle Vision requests (default 30)
          --body-mode 2d|3d           realtime 2D default; opt-in Vision 3D
          --segmentation-every N       refresh person matte every N tracking frames (default 3)
          --max-frames N              exit after N emitted camera frames
          --frame-share-name NAME     publish BGRA + person matte to local POSIX shm
          --mock                      emit one deterministic TrackingFrame
          --help                      show this help

        Camera mode writes one TrackingFrame JSON object per line to stdout.
        Diagnostics go to stderr. No network access is used.
        """)
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("error: \(message)\n".utf8))
        exit(1)
    }
}

private struct CaptureRuntime {
    let session: AVCaptureSession
    let tracker: VisionTracker
}

private struct BridgeError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}
