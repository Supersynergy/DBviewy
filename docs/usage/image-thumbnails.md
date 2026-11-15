# Image thumbnails and the image viewer

Choose the thumbnail size under **Settings > Table**. Cells containing an image address or a SQLite image attachment show previews as they enter the visible grid. Web previews are cached on disk. For file paths in a sheet, open the image viewer and choose the folder containing the images when prompted.

In a writable SQLite table, right-click a cell and select **Import image...**. Choose a local file or paste an HTTP or HTTPS address and select **Save**. DBviewy opens the **Bildimporte** table. The image and its compressed thumbnail travel with the database. Repeat imports of the same bytes and source reuse the same entry.

## Browse records and pictures

Click a thumbnail or press **Shift+F6** on any row. The viewer follows the current sort and filter and includes records without pictures.

| Key | Action |
| --- | --- |
| Up / Down or Page Up / Page Down | Previous or next record |
| Left / Right or Shift+Space / Space | Previous or next picture, then the neighbouring record |
| Home / End | First or last record |
| + / - | Zoom in or out |
| 0 | Fit to window |
| D | Toggle record details |
| T | Choose a SQLite table with Up/Down and Enter |
| Esc | Return to the last viewed record in the grid |

Drag the window edge to resize, drag a zoomed picture to pan, or double-click it to switch between fit and its pixel size.
