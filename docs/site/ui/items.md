---
title: Visual items, rectangles and images
description: Build visual trees with Item, draw boxes with Rectangle, transform items and load pictures with Image, BorderImage and AnimatedImage.
section: ui
order: 1
---
# Visual items, rectangles and images

Use `Item` as the base of every visual element in your user interface, `Rectangle` when you need a coloured or bordered box, and `Image`, `BorderImage` or `AnimatedImage` when you display pictures, nine-patch borders, SVGs or PDF pages.

## Item geometry, visibility and stacking

`Item` provides the plain properties that every visual item shares: geometry (`x`, `y`, `width`, `height`, `implicitWidth`, `implicitHeight`), `z`, `visible`, `opacity`, `clip`, `enabled`, `rotation`, `scale`, `transformOrigin`, `transform`, `layer`, `antialiasing` and `smooth`. Setting a property that an item does not have is rejected.

Geometry reads and writes like any other property. If you bind a child's `width` to `parent.width / 2`, the child follows the parent when the parent resizes. `childrenRect` covers every child of an item, including children at negative positions, though a transformed child does not change its parent's `childrenRect`.

A new item is `visible` by default, and you can set `visible` to `false` and back to `true`. Hiding a parent makes its children read as not visible; showing the parent again restores them, except for any child that was hidden on its own. `enabled` also passes down the tree, so disabling a parent disables its children. `opacity` applies visually to children, but each item keeps its own property value: a child inside a parent with `opacity: 0.5` still reads `1`. `opacity` is stored clamped between `0` and `1`—writing `3` stores `1`, and a change handler hears nothing if the clamped value is the one it already had.

Setting `clip: true` cuts an item's drawing to its bounds. An item with `visible: false`, or one outside its clip or viewport, is not painted.

```qml
import QtQuick

Item {
    width: 240
    height: 160

    Rectangle {
        x: 10; y: 10
        width: parent.width / 2
        height: 60
        color: "#2060a0"
        radius: 4
        z: 1
    }

    Item {
        x: 20; y: 40
        width: 100; height: 100
        clip: true
        opacity: 0.5

        Rectangle {
            x: 60; y: 60
            width: 80; height: 80
            color: "#c03030"
        }
    }

    Rectangle {
        x: 150; y: 10
        width: 60; height: 40
        visible: false
        color: "#00ff00"
    }
}
```

This draws a blue rounded bar across the top left, clips a red square at the right and bottom edges of a half-opaque box, and leaves the hidden green box unpainted.

Watch out for a few pitfalls with visibility and stacking:
- Hiding an item (`visible: false`) stops it from being painted, but its bindings keep re-evaluating and its timers keep running.
- Children are painted in order of `z`, and children with the same `z` are painted in declaration order. The order is stable and re-sorted when a child's `z` or the children list changes.
- `childAt(x, y)` returns the visible child under a point (in the item's own coordinates, through the child's transform) or `null`, but it goes by declaration order rather than `z`: the last child declared under the point wins. A visible child with `opacity: 0` still counts, whereas a hidden child does not.

```qml
import QtQuick

Item {
    id: root
    width: 200; height: 200

    function nameAt(x, y) {
        const c = root.childAt(x, y)
        return c === null ? "none" : c.objectName
    }

    Rectangle { objectName: "raised"; width: 100; height: 100; color: "#333333"; z: 1 }
    Rectangle { objectName: "under"; width: 100; height: 100; color: "#666666" }

    Component.onCompleted: console.log("hit: " + nameAt(10, 10))
}
```

Here the first grey box is painted on top because of `z: 1`, but `childAt(10, 10)` returns the box declared last (`hit: under`).

## Rectangle, corners and antialiasing

`Rectangle` paints a filled box with an optional border and rounded corners. Its defaults are `color: "#ffffff"`, `radius: 0`, `border.width: 1` and `border.color: "#000000"`, where `border` is a grouped `RectangleBorder` property. A `Rectangle` reports its colour as lowercase hex, so `"#0000FF"` reads back as `"#0000ff"`.

You can set `radius` to a value larger than the box, or round individual corners with `topLeftRadius`, `topRightRadius`, `bottomLeftRadius` and `bottomRightRadius`. Each corner defaults to `radius`: a corner that has not been given its own value follows `radius` when `radius` is bound or changed later, while a corner given an explicit value keeps it when `radius` changes.

`antialiasing` is `false` for a plain `Item` and for a `Rectangle` with `radius: 0`, and `true` for a `Rectangle` with a radius or corner radius (as well as for `Text`). Once you assign a value to `antialiasing`, your value stays even if the radius changes. `smooth` is `true` on every item by default and can be set to `false` on a `Rectangle`.

## Transforms, coordinate mapping, Flipable and layers

Every item has `rotation`, `scale`, `transformOrigin` (`Item.TopLeft`, `Top`, `TopRight`, `Left`, `Center`, `Right`, `BottomLeft`, `Bottom`, `BottomRight`) and a `transform` list. Entries in `transform` are applied in order after the item's own `rotation` and `scale`. The available transform types are:
- `Translate` (`x`, `y`), starting at `0`.
- `Rotation` (`angle`, `axis`, `origin`), starting with axis z `1` and angle `0`. A rotation about the x or y axis is drawn in perspective.
- `Scale` (`xScale`, `yScale`, `zScale`, `origin`), starting with scales `1`.
- `Shear` (`xFactor`, `yFactor`, `xAngle`, `yAngle`, `origin`), starting with factors `0`.
- `Matrix4x4` (`matrix`).

Changing a transform moves where the item is drawn, and pointer clicks land on the transformed position. To convert points between items and the desktop, call `mapToItem`, `mapFromItem`, `mapToGlobal` and `mapFromGlobal`.

```qml
import QtQuick

Item {
    width: 200
    height: 200

    Rectangle {
        x: 20; y: 20
        width: 60; height: 30
        color: "#2060a0"
        transform: [
            Translate { x: 10 },
            Rotation { angle: 45; origin.x: 30; origin.y: 15 },
            Scale { xScale: 1.5 }
        ]
    }
}
```

The rectangle moves 10 pixels to the right, turns 45 degrees about its centre, and stretches 1.5 times across; each transform applies to the result of the one before it.

`Flipable` shows its `front` or `back` item according to which side its transform faces (`side` is `Flipable.Front` (0) or `Flipable.Back` (1)). A 2D `rotation` of 180 still faces `Front`, whereas a `Scale` with `xScale: -1` faces `Back`, a rotation of 100 degrees about the x axis faces `Back`, and a rotation of 60 degrees about the y axis faces `Front`.

Every item also has a `layer` group (`ItemLayer`) with defaults `enabled: false`, `live: true`, `smooth: false` and `format: 1`, alongside `mipmap`, `samplerName`, `textureSize`, `sourceRect`, `samples`, `wrapMode`, `textureMirroring` and `effect`. Setting `layer.enabled: true` allows a `ShaderEffect` to sample the item as a picture; an item without a layer gives a sampler nothing. <!-- check: layer.enabled sampling by ShaderEffect is marked @ljos-only -->

## Loading and drawing images

`Image` loads a picture from `source`. For a local file, `status` becomes `Image.Ready` (1) with its implicit size as soon as `source` is set, and the `Image` is as large as its picture unless you set `width` or `height`. An empty `source` gives `Image.Null` (0) with implicit size 0. A missing file gives `Image.Error` (3), logs a `Cannot open` warning and sets implicit size to 0; a file that is not an image gives `Image.Error` and warns `Unsupported image format`. Setting a valid source afterwards clears the error.

For background and network loading:
- Setting `asynchronous: true` reads and decodes a local file away from the user interface. While it loads, `status` is `Image.Loading` (2) and the old size stays until the new picture is ready.
- A network URL starts in `Image.Loading` even when `asynchronous` is `false`, and updates its size when the picture arrives. If the server does not have the file, the error reports the server's reply.
- A large local picture (over 512 by 512 pixels) reports `Ready` at once after reading only its header; its pixels are decoded on another thread at the drawn size and painted a moment later. <!-- check: large local picture background pixel decode after Ready is marked @ljos-only -->

```qml
import QtQuick

Item {
    width: 300
    height: 200

    Image {
        id: photo
        anchors.fill: parent
        source: "photo.jpg"
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        Accessible.ignored: true
        sourceSize.width: 600
    }

    Text {
        anchors.centerIn: parent
        visible: photo.status === Image.Loading
        text: "Loading"
    }
}
```

Here the photo decodes away from the user interface with `Accessible.ignored: true` for decoration, shows `"Loading"` while `photo.status === Image.Loading`, and then fills the box with `Image.PreserveAspectCrop`.

Use `sourceSize` to ask for a decode size. Setting `sourceSize.width` and leaving `height` unset computes the other side from the picture's aspect ratio. `sourceSize` is kept in whole pixels, with half-pixels rounded away from zero. When a large picture is drawn in a small or unpainted item, its decoded pixels are kept at about the drawn size while `sourceSize` and the implicit size stay those of the picture. <!-- check: keeping decoded pixels at drawn size is marked @ljos-only -->

`fillMode` controls how the picture is placed inside the item's box (`Image.Stretch`, `Image.PreserveAspectFit`, `Image.PreserveAspectCrop`, `Image.Tile`, `Image.TileVertically`, `Image.TileHorizontally`, `Image.Pad`), aligned with `horizontalAlignment` (`AlignLeft`, `AlignRight`, `AlignHCenter`) and `verticalAlignment` (`AlignTop`, `AlignBottom`, `AlignVCenter`). In a 100 by 100 box, a 2:1 picture paints at 100 by 50 (`paintedWidth` by `paintedHeight`) with `PreserveAspectFit`, 200 by 100 with `PreserveAspectCrop`, and its own size with `Pad`. Setting `mirror`, `mirrorVertically` or `cache` does not change `implicitWidth` or `paintedWidth`.

For other image formats and items:
- **Still and multi-frame images**: a still picture has `frameCount: 1` and `currentFrame: 0`; setting `currentFrame` to a frame the picture does not have is ignored.
- **SVG**: an SVG in `Image` is drawn at the size the `Image` asks for, or at its viewBox size when neither `width` nor `height` is set.
- **PDF**: when built with the `pdf` feature (which the runner enables), a PDF document loads as an `Image` where `frameCount` is the page count and each page is the size of its media box rounded to whole pixels. Without the `pdf` feature, a PDF is treated as an unsupported format.
- **`BorderImage`**: cuts its picture into nine parts using `border.left`, `border.top`, `border.right` and `border.bottom`, and tiles the edges and centre using `horizontalTileMode` and `verticalTileMode` (`BorderImage.Stretch` (0), `BorderImage.Repeat` (1), `BorderImage.Round` (2)). It loads a local file at once and uses the picture's size as its implicit size unless a size is set.
- **`AnimatedImage`**: plays the frames of an animated GIF for the duration each frame sets in the file, controlled by `playing`, `paused` and `speed`, announcing each change of frame.

## Differences from Qt

- A missing picture is warned about once per source rather than once per `Image` item.
- A large local picture (more than 512 by 512 pixels) is decoded on another thread at its drawn size, so its pixels arrive a moment after its `status` becomes `Ready`.
- An item's `layer.effect` property is kept, but the effect is not drawn; the item draws itself instead. <!-- check: layer.effect behaviour is marked @ljos-only -->
- Passing `NaN` to `x`, `y`, `width`, `height`, `rotation`, `scale`, `opacity` or `z` is not yet ignored. <!-- check: geometry NaN handling scenarios are @backlog -->
- An invalid colour string reads as `#000000` with `valid` false, and a `Rectangle` given one paints black. <!-- check: the scenario that a Text's black is a valid colour is @backlog -->
- `childAt` ignores `z` and returns the last child declared under the point, whereas painting orders children by `z`.
- A file named as a PDF that is not a valid PDF document reports an error with a warning and draws nothing.

## See also

- [Anchors and positioners](./positioning.md)
- [Layouts](./layouts.md)
- [Shapes, gradients, canvas and effects](./shapes-and-effects.md)
- [Offscreen items, virtualisation and cost](./offscreen-and-cost.md)
