# Image Thumbnails and Viewer

Cells that hold an image URL (ending in .png, .jpg, .webp and friends) can
show the picture itself instead of the address. This is off by default;
turn it on under **Settings > Table > Image thumbnails** and pick a size:
Small (20 px), Medium (32 px) or Large (48 px).

## What the table does

- Only cells whose text is a bare image URL get a thumbnail. A URL inside a
  sentence stays plain text, and clicking it still opens the link.
- Only pictures on screen are fetched, on a background thread, so a table
  with a million image URLs never downloads them all. A finished fetch
  appears on the next repaint.
- The cache holds a bounded number of pictures and forgets the oldest
  first, so memory stays flat while scrolling.
- **Off** means nothing is fetched automatically. The **View image** cell
  menu entry still works and downloads only the one picture you open.

## The image viewer

Right-click an image cell and choose **View image** (this works even with
thumbnails off), or click a thumbnail.
The viewer collects every image-bearing cell of the tab, in the order the
current filter and sort show them, so the arrow keys walk the sheet's
pictures.

| Key                     | Action                                                            |
|-------------------------|-------------------------------------------------------------------|
| Left / Up / PageUp      | Previous picture                                                  |
| Right / Down / PageDown | Next picture                                                      |
| + / -                   | Zoom in steps (up to 16x) / zoom out                              |
| 0                       | Fit the picture to the window again                               |
| Esc                     | Close                                                             |

Scroll bars or a drag pan a zoomed picture. **Open URL** hands the image
address to your browser. The keys do nothing while a text field (SQL panel,
chat input) owns the keyboard.

The details column shows the cell's row: column name, position, URL, image
size and the neighbouring cells' values.

Neighbouring pictures are fetched ahead of the arrow keys, so stepping
through a sheet feels instant once the first one landed.
