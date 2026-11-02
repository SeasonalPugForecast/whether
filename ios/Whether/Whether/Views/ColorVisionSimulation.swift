#if DEBUG
    import CoreImage
    import CoreImage.CIFilterBuiltins
    import UIKit

    /// `-simulate protan|deutan|tritan|achromat` (screenshots): a window over
    /// the app showing the screen as a reader with that color vision sees it, with the
    /// matrices the tests check the themes with (`ColorMath.simulationMatrix`, Machado et
    /// al. 2009 at full severity), applied by Core Image in its linear working space. It
    /// redraws twice a second from a snapshot of the app's window (the map included) and
    /// lets every touch through. A label at the bottom says which simulation it is.
    @MainActor
    enum ColorVisionSimulation {
        private static var window: UIWindow?
        private static var imageView: UIImageView?
        private static let context = CIContext()

        /// Starts the simulation when the launch asks for one.
        static func startIfAsked() {
            let args = ProcessInfo.processInfo.arguments
            guard window == nil, let i = args.firstIndex(of: "-simulate"), i + 1 < args.count,
                let deficiency = Deficiency(rawValue: args[i + 1]),
                let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first
            else { return }
            let overlay = UIWindow(windowScene: scene)
            overlay.windowLevel = .alert + 1
            overlay.isUserInteractionEnabled = false
            overlay.backgroundColor = .clear
            let controller = UIViewController()
            controller.view.backgroundColor = .clear
            let image = UIImageView(frame: overlay.bounds)
            image.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            image.contentMode = .scaleToFill
            controller.view.addSubview(image)
            let badge = UILabel()
            badge.text = "  Simulated: \(deficiency.rawValue)  "
            badge.font = .preferredFont(forTextStyle: .caption2)
            badge.textColor = .white
            badge.backgroundColor = UIColor.black.withAlphaComponent(0.65)
            badge.layer.cornerRadius = 7
            badge.layer.masksToBounds = true
            badge.translatesAutoresizingMaskIntoConstraints = false
            controller.view.addSubview(badge)
            NSLayoutConstraint.activate([
                badge.centerXAnchor.constraint(equalTo: controller.view.centerXAnchor),
                badge.bottomAnchor.constraint(equalTo: controller.view.bottomAnchor, constant: -4),
            ])
            overlay.rootViewController = controller
            overlay.isHidden = false
            window = overlay
            imageView = image
            Task {
                while !Task.isCancelled {
                    redraw(deficiency, scene: scene)
                    try? await Task.sleep(for: .milliseconds(500))
                }
            }
        }

        /// Snapshots the app's window and shows it through the simulation.
        private static func redraw(_ deficiency: Deficiency, scene: UIWindowScene) {
            guard let imageView,
                let app = scene.keyWindow ?? scene.windows.first(where: { $0 !== window && !$0.isHidden })
            else { return }
            let shot = UIGraphicsImageRenderer(bounds: app.bounds).image { _ in
                _ = app.drawHierarchy(in: app.bounds, afterScreenUpdates: false)
            }
            guard let input = CIImage(image: shot) else { return }
            let m = ColorMath.simulationMatrix(deficiency)
            let filter = CIFilter.colorMatrix()
            filter.inputImage = input
            filter.rVector = CIVector(x: m[0][0], y: m[0][1], z: m[0][2], w: 0)
            filter.gVector = CIVector(x: m[1][0], y: m[1][1], z: m[1][2], w: 0)
            filter.bVector = CIVector(x: m[2][0], y: m[2][1], z: m[2][2], w: 0)
            filter.aVector = CIVector(x: 0, y: 0, z: 0, w: 1)
            filter.biasVector = CIVector(x: 0, y: 0, z: 0, w: 0)
            guard let output = filter.outputImage?.applyingFilter("CIColorClamp"),
                let image = context.createCGImage(output, from: input.extent)
            else { return }
            imageView.image = UIImage(cgImage: image, scale: shot.scale, orientation: .up)
        }
    }
#endif
