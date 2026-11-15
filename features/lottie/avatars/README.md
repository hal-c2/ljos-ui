# HAL-C2 avatar animations

The two avatar animations of [hal-c2/lottie-animation](https://github.com/hal-c2/lottie-animation),
`version2/`, at commit `1a9dbdae376677f8162ad04d8db12e3ab94484d7`, under its MIT licence (the
same as Ljós's).

| File | What it is |
|---|---|
| `hal-c2-dance.json` | the dance: 24 frames (1 to 24) at 24 fps, 512 by 512, 4 layers; it sways left and right once a second and loops seamlessly |
| `hal-c2-genie.json` | the genie: 47 frames (1 to 47) at 24 fps, 512 by 512, 6 layers; it grows out of nothing (frame 1 is empty) into the face by frame 31 and holds it |
| `dance-NN.png`, `genie-NN.png` | frame `NN` of the file, taken from the preview GIF exported beside it (`hal-c2-dance-preview.gif`, `hal-c2-genie-preview.gif`) |

Both JSON files are as the repository has them, byte for byte. Blender's grease pencil drew
them and its Lottie add-on exported them (Lottie 5.5.6); `version2/build_hal_c2_lottie.py`
there builds them.

The preview GIFs are 320 by 320, a frame every 40 ms. GIF frame `i` (from 0) is the file's
frame `i + 1`. The genie GIF's last frame stands for frames 41 to 47, which are all the same
picture, so it is shown for 290 ms. Each PNG here is one GIF frame, laid over the GIF's
white and written as an indexed PNG with the GIF frame's own colours, nothing else changed.
The `.blend` files, the `.lottie` archives and the GIFs themselves are not kept.

`features/lottie/lottie-avatars.feature` compares Ljós's frames with these pictures.
