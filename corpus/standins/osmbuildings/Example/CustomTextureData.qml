// Stands in for CustomTextureData (manager.cpp): a picture made of the bytes
// of an image file, here the PNG a tile of the map is.
//
// In C++ QImage reads the file there and then. Here it is read away from the
// UI, and is done a little later: until then the picture is what it was.
import QtQuick
import QtQuick3D

TextureData {
    id: textureData

    function setImageData(data) {
        if (data)
            textureData.setImageFileData(data)
    }
}
