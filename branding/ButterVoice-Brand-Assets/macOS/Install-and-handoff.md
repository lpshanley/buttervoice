# macOS assets

## App icon

- **ButterVoice.icns** contains ten native Mac size and display-scale representations, spanning 16 through 1024 pixels. Use this file for a classic macOS icon bundle. The existing Tauri project can reference it at its configured macOS bundle icon path.
- **AppIcon.appiconset/** contains equivalent PNGs with the asset-catalog metadata. Import the whole folder into an Xcode asset catalog.
- **Layers/** contains the editable two-layer layout, a full-bleed flattened PNG, and separate 1024 px porcelain and transparent Ghee PNGs for authoring a layered icon. Both PNG layers already share the same canvas and alignment. Import them at their original dimensions.
- For the full-bleed design, Ghee occupies 74% of the visible porcelain tile width. The classic ICNS uses an inset tile on a transparent square canvas. Place that complete image as provided. Adding another background or mask would introduce a second tile.

## Menu bar

Use **MenuBar/GheeTemplate.png** and **GheeTemplate@2x.png** as a template image. The canvases are 22 × 22 and 44 × 44 pixels; the drawing is about 18 logical points tall. Mark the image as a template in the app so the system supplies the foreground color. A Tauri app with a dedicated menu-bar asset can keep its full-color app icon separately. **GheeTemplate.svg** is the editable drawing on the same 22-unit grid.

The shaded mascot in this release comes from the approved raster artwork. The outline, face, and lettering are editable vectors. The full-bleed layered SVG separates editable placement, the porcelain base, and the embedded mascot.
