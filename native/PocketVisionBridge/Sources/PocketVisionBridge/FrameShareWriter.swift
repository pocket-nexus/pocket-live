import CoreVideo
import CFrameShare
import Darwin
import Foundation

private let frameShareMagic: [UInt8] = [0x50, 0x4B, 0x4C, 0x56, 0x46, 0x52, 0x4D, 0x00]
private let frameShareVersion: UInt32 = 1
private let frameShareHeaderBytes = 64

final class FrameShareWriter {
    private let fd: Int32
    private let mapping: UnsafeMutableRawPointer
    private let capacity: Int

    init(name: String) throws {
        fd = name.withCString(pocket_shm_open_readwrite)
        guard fd >= 0 else { throw FrameShareError("shm_open \(name): \(errno)") }
        var info = stat()
        guard fstat(fd, &info) == 0 else {
            Darwin.close(fd)
            throw FrameShareError("fstat frame share: \(errno)")
        }
        capacity = Int(info.st_size)
        let mapped = mmap(nil, capacity, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0)
        guard mapped != MAP_FAILED, let mapped else {
            Darwin.close(fd)
            throw FrameShareError("mmap frame share: \(errno)")
        }
        mapping = mapped
    }

    deinit {
        munmap(mapping, capacity)
        Darwin.close(fd)
    }

    func write(
        camera: CVPixelBuffer,
        personMask: CVPixelBuffer?,
        sequence: UInt64,
        capturedAtNs: UInt64
    ) throws {
        guard CVPixelBufferGetPixelFormatType(camera) == kCVPixelFormatType_32BGRA else {
            throw FrameShareError("camera buffer is not BGRA8")
        }
        let width = CVPixelBufferGetWidth(camera)
        let height = CVPixelBufferGetHeight(camera)
        let packedStride = width * 4
        let bgraBytes = packedStride * height

        var maskWidth = 0
        var maskHeight = 0
        var maskBytes = 0
        if let personMask {
            guard CVPixelBufferGetPixelFormatType(personMask) == kCVPixelFormatType_OneComponent8 else {
                throw FrameShareError("person mask is not OneComponent8")
            }
            maskWidth = CVPixelBufferGetWidth(personMask)
            maskHeight = CVPixelBufferGetHeight(personMask)
            maskBytes = maskWidth * maskHeight
        }
        guard frameShareHeaderBytes + bgraBytes + maskBytes <= capacity else {
            throw FrameShareError("frame \(width)x\(height) exceeds shared-memory capacity")
        }

        CVPixelBufferLockBaseAddress(camera, .readOnly)
        if let personMask { CVPixelBufferLockBaseAddress(personMask, .readOnly) }
        defer {
            if let personMask { CVPixelBufferUnlockBaseAddress(personMask, .readOnly) }
            CVPixelBufferUnlockBaseAddress(camera, .readOnly)
        }
        guard let cameraBase = CVPixelBufferGetBaseAddress(camera) else {
            throw FrameShareError("camera buffer has no base address")
        }
        let cameraStride = CVPixelBufferGetBytesPerRow(camera)

        // POSIX shm on macOS does not support flock. Publish with a tiny
        // cross-process sequence lock instead: 0 while bytes are changing,
        // then the real sequence with release ordering when complete.
        pocket_frame_publish_begin(mapping)

        let bgraDestination = mapping.advanced(by: frameShareHeaderBytes)
        for row in 0..<height {
            memcpy(
                bgraDestination.advanced(by: row * packedStride),
                cameraBase.advanced(by: row * cameraStride),
                packedStride
            )
        }
        if let personMask,
           let maskBase = CVPixelBufferGetBaseAddress(personMask)
        {
            let maskStride = CVPixelBufferGetBytesPerRow(personMask)
            let maskDestination = bgraDestination.advanced(by: bgraBytes)
            for row in 0..<maskHeight {
                memcpy(
                    maskDestination.advanced(by: row * maskWidth),
                    maskBase.advanced(by: row * maskStride),
                    maskWidth
                )
            }
        }

        _ = frameShareMagic.withUnsafeBytes { source in
            memcpy(mapping, source.baseAddress!, frameShareMagic.count)
        }
        put(frameShareVersion, at: 8)
        put(UInt32(frameShareHeaderBytes), at: 12)
        put(capturedAtNs, at: 24)
        put(UInt32(width), at: 32)
        put(UInt32(height), at: 36)
        put(UInt32(packedStride), at: 40)
        put(UInt32(bgraBytes), at: 44)
        put(UInt32(maskWidth), at: 48)
        put(UInt32(maskHeight), at: 52)
        put(UInt32(maskWidth), at: 56)
        put(UInt32(maskBytes), at: 60)
        pocket_frame_publish_end(mapping, sequence)
    }

    private func put(_ value: UInt32, at offset: Int) {
        mapping.storeBytes(of: value.littleEndian, toByteOffset: offset, as: UInt32.self)
    }

    private func put(_ value: UInt64, at offset: Int) {
        mapping.storeBytes(of: value.littleEndian, toByteOffset: offset, as: UInt64.self)
    }
}

private struct FrameShareError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}
