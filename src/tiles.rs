//! Tile maps: the reusable world layer beneath sprites and actors.
//!
//! A [`TileSet`] slices one sheet [`Sprite`] into a uniform grid of numbered
//! tiles (plus optional game-opaque per-tile flags). A [`TileLayer`] is a
//! row-major field of `Option<`[`TileId`]`>` cells over one tileset — `None` is
//! an explicit empty cell, not a magic zero. A [`TileMap`] stacks layers over a
//! shared cell grid. [`TileGrid`] is the pure layout: it turns a `(col, row)`
//! tile coordinate into the virtual-pixel [`Rect`] it occupies (and centres a
//! sprite within a tile). [`TileMapView`] ties them together as a [`PixelLayer`]:
//! it resolves each cell's tile against a game-supplied [`TileSets`] collection
//! and blits the tile's sheet region through the crisp integer
//! [`Surface::draw_sprite_ex`] path.
//!
//! The split keeps the toolkit windowing- and game-agnostic: maps and layers are
//! plain serde data (author them in JSON), tilesets hold pixels (so they are
//! built in code, not deserialised), and the renderer only ever calls the
//! existing [`Surface`] API. Games layer their own actors — a player, pickups —
//! *over* the tile view; the tiles are the static backdrop.

use crate::geometry::{Point, Rect, Size};
use crate::present::PixelLayer;
use crate::sprite::Sprite;
use crate::surface::{BlitOptions, Surface};

/// A tile's index within its [`TileSet`] — 0-based and *local to that tileset*
/// (two tilesets each have their own tile `0`). Which tileset a cell resolves
/// against is the [`TileLayer`]'s [`TileSetId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileId(u32);

impl TileId {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// The underlying index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Selects one [`TileSet`] within a [`TileSets`] collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileSetId(u32);

impl TileSetId {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// The underlying index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One sheet [`Sprite`] sliced into a uniform grid of tiles, laid out
/// left-to-right then top-to-bottom over `columns` columns. Each tile carries an
/// optional game-opaque `u8` flag byte the toolkit never interprets (collision,
/// terrain kind, whatever the game wants). Holds pixels, so it is built in code
/// — not `serde` (author the *maps* as data; supply the *tilesets* at render
/// time).
#[derive(Debug, Clone)]
pub struct TileSet {
    sheet: Sprite,
    tile_size: Size,
    columns: u32,
    /// Per-tile flags, indexed by [`TileId`]; empty means "all zero".
    flags: Vec<u8>,
}

impl TileSet {
    /// Slice `sheet` into `tile_size` tiles over `columns` columns. A tile is
    /// addressable only while its whole region lands inside the sheet, so a
    /// short final row simply exposes fewer tiles. A zero `tile_size` or
    /// `columns` yields an empty tileset (every lookup misses) rather than
    /// dividing by zero.
    #[must_use]
    pub fn grid(sheet: Sprite, tile_size: Size, columns: u32) -> Self {
        Self {
            sheet,
            tile_size,
            columns,
            flags: Vec::new(),
        }
    }

    /// Attach per-tile flag bytes (indexed by [`TileId`]); ids past the end read
    /// back `0`.
    #[must_use]
    pub fn with_flags(mut self, flags: Vec<u8>) -> Self {
        self.flags = flags;
        self
    }

    /// The uniform source tile size.
    #[must_use]
    pub fn tile_size(&self) -> Size {
        self.tile_size
    }

    /// The backing sheet, for the blit.
    #[must_use]
    pub fn sheet(&self) -> &Sprite {
        &self.sheet
    }

    /// How many whole tiles the sheet holds at this grid.
    #[must_use]
    pub fn len(&self) -> usize {
        if self.tile_size.w == 0 || self.tile_size.h == 0 || self.columns == 0 {
            return 0;
        }
        let rows = self.sheet.size().h / self.tile_size.h;
        (self.columns * rows) as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The sub-rectangle of the sheet holding tile `id`, or `None` when the tile
    /// is out of range or its region would fall outside the sheet.
    #[must_use]
    pub fn region(&self, id: TileId) -> Option<Rect> {
        if self.tile_size.w == 0 || self.tile_size.h == 0 || self.columns == 0 {
            return None;
        }
        let index = id.get();
        let (col, row) = (index % self.columns, index / self.columns);
        let origin = Point::new(
            (col * self.tile_size.w) as i32,
            (row * self.tile_size.h) as i32,
        );
        let sheet = self.sheet.size();
        let fits = origin.x as u32 + self.tile_size.w <= sheet.w
            && origin.y as u32 + self.tile_size.h <= sheet.h;
        fits.then_some(Rect::new(origin, self.tile_size))
    }

    /// Tile `id`'s flag byte (`0` when it has none).
    #[must_use]
    pub fn flags(&self, id: TileId) -> u8 {
        self.flags.get(id.get() as usize).copied().unwrap_or(0)
    }
}

/// The game-supplied collection a [`TileMapView`] resolves [`TileSetId`]s
/// against — one tileset per id, in order.
#[derive(Debug, Clone, Default)]
pub struct TileSets {
    sets: Vec<TileSet>,
}

impl TileSets {
    #[must_use]
    pub fn new(sets: Vec<TileSet>) -> Self {
        Self { sets }
    }

    /// The tileset for `id`, or `None` when the id is out of range.
    #[must_use]
    pub fn get(&self, id: TileSetId) -> Option<&TileSet> {
        self.sets.get(id.get() as usize)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }
}

/// One row-major field of tile cells over a single tileset. `None` is an empty
/// cell (the backdrop shows through); `Some(id)` paints tile `id` of `tileset`.
/// Plain data — author it in JSON. Its length is validated against the owning
/// [`TileMap`]'s grid, not here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TileLayer {
    /// Which tileset (in the render-time [`TileSets`]) this layer's ids index.
    pub tileset: TileSetId,
    /// Row-major cells; `None` is empty.
    pub cells: Vec<Option<TileId>>,
}

impl TileLayer {
    #[must_use]
    pub fn new(tileset: TileSetId, cells: Vec<Option<TileId>>) -> Self {
        Self { tileset, cells }
    }
}

/// A stack of [`TileLayer`]s over a shared `size` (columns × rows) cell grid,
/// drawn bottom layer first. Serde data; validate a deserialised map with
/// [`validate`](Self::validate) before rendering (the derived `Deserialize`
/// cannot enforce the per-layer cell count on its own).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TileMap {
    size: Size,
    layers: Vec<TileLayer>,
}

impl TileMap {
    /// A validated map: non-zero `size`, and every layer holding exactly
    /// `size.area()` cells.
    ///
    /// # Errors
    /// [`TileMapError`] on a zero dimension or a layer whose cell count does not
    /// match the grid.
    pub fn new(size: Size, layers: Vec<TileLayer>) -> Result<Self, TileMapError> {
        let map = Self { size, layers };
        map.validate()?;
        Ok(map)
    }

    /// Re-check the invariants of a (possibly deserialised) map.
    ///
    /// # Errors
    /// [`TileMapError`] on a zero dimension or a per-layer cell-count mismatch.
    pub fn validate(&self) -> Result<(), TileMapError> {
        if self.size.w == 0 || self.size.h == 0 {
            return Err(TileMapError::ZeroSize);
        }
        let expected = self.size.area();
        for (layer, cells) in self.layers.iter().enumerate() {
            if cells.cells.len() != expected {
                return Err(TileMapError::CellCountMismatch {
                    layer,
                    expected,
                    got: cells.cells.len(),
                });
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    #[must_use]
    pub fn layers(&self) -> &[TileLayer] {
        &self.layers
    }
}

/// Why a [`TileMap`] is malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TileMapError {
    /// The cell grid has a zero dimension.
    #[error("a tile map needs non-zero width and height")]
    ZeroSize,
    /// A layer's cell count does not match the `expected` grid area.
    #[error("layer {layer} has {got} cells but the grid needs {expected}")]
    CellCountMismatch {
        layer: usize,
        expected: usize,
        got: usize,
    },
}

/// The pure tile↔pixel layout: an `origin` in virtual pixels, a `tile_size`, and
/// the grid extent in tiles. Turns a `(col, row)` tile coordinate into the
/// virtual-pixel [`Rect`] it fills, and centres a sprite within a tile. Holds no
/// pixels and no cells — just the geometry a [`TileMapView`] (and a game's own
/// actors) share so tiles and actors line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileGrid {
    origin: Point,
    tile_size: Size,
    size: Size,
}

impl TileGrid {
    /// A grid pinned at `origin`, `tile_size` per tile, `size` tiles across and
    /// down.
    #[must_use]
    pub fn new(origin: Point, tile_size: Size, size: Size) -> Self {
        Self {
            origin,
            tile_size,
            size,
        }
    }

    /// A grid of `size` tiles centred within `area` (integer offsets, so a grid
    /// larger than `area` overhangs symmetrically and clips at the surface, like
    /// any off-edge blit). The reusable half of "centre the maze in the band
    /// below the HUD": the caller frames `area`, the grid centres in it.
    #[must_use]
    pub fn centered_in(area: Rect, tile_size: Size, size: Size) -> Self {
        let pixels = grid_pixels(tile_size, size);
        let origin = Point::new(
            area.origin.x + (area.size.w as i32 - pixels.w as i32) / 2,
            area.origin.y + (area.size.h as i32 - pixels.h as i32) / 2,
        );
        Self::new(origin, tile_size, size)
    }

    #[must_use]
    pub fn origin(&self) -> Point {
        self.origin
    }

    #[must_use]
    pub fn tile_size(&self) -> Size {
        self.tile_size
    }

    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    /// The grid's total extent in virtual pixels (`size` × `tile_size`).
    #[must_use]
    pub fn pixel_size(&self) -> Size {
        grid_pixels(self.tile_size, self.size)
    }

    /// The virtual-pixel rect of the tile at `(col, row)`. Not bounds-checked
    /// against `size` — an out-of-range tile simply lands past the grid.
    #[must_use]
    pub fn tile_rect(&self, col: u32, row: u32) -> Rect {
        Rect::new(
            Point::new(
                self.origin.x + (col * self.tile_size.w) as i32,
                self.origin.y + (row * self.tile_size.h) as i32,
            ),
            self.tile_size,
        )
    }

    /// Where a sprite of `sprite_size` sits centred within the `(col, row)` tile
    /// (signed, so a sprite larger than the tile overhangs symmetrically).
    #[must_use]
    pub fn centered_in_tile(&self, col: u32, row: u32, sprite_size: Size) -> Point {
        let rect = self.tile_rect(col, row);
        Point::new(
            rect.origin.x + (self.tile_size.w as i32 - sprite_size.w as i32) / 2,
            rect.origin.y + (self.tile_size.h as i32 - sprite_size.h as i32) / 2,
        )
    }
}

/// `tile_size` scaled up by a `size`-tile grid.
fn grid_pixels(tile_size: Size, size: Size) -> Size {
    Size::new(size.w * tile_size.w, size.h * tile_size.h)
}

/// A [`TileMap`] drawn as a [`PixelLayer`]: each `Some` cell blits its tile's
/// sheet region at the cell's [`TileGrid::tile_rect`], integer-scaled to fill
/// the grid tile. Layers paint in order; empty cells and unresolved tilesets are
/// skipped.
///
/// The fill scale is `grid.tile_size / tileset.tile_size`, floored to stay a
/// crisp integer — so a grid tile should be a whole multiple of the source tile
/// (e.g. an 8px sheet tile at a 16px grid draws 2×). The map's `size` and the
/// grid's `size` are expected to match; the view decomposes cells by the map's
/// width.
pub struct TileMapView<'a> {
    map: &'a TileMap,
    grid: &'a TileGrid,
    tilesets: &'a TileSets,
}

impl<'a> TileMapView<'a> {
    #[must_use]
    pub fn new(map: &'a TileMap, grid: &'a TileGrid, tilesets: &'a TileSets) -> Self {
        Self {
            map,
            grid,
            tilesets,
        }
    }
}

impl PixelLayer for TileMapView<'_> {
    fn render(&self, screen: &mut Surface) {
        let cols = self.map.size().w;
        if cols == 0 {
            return;
        }
        for layer in self.map.layers() {
            let Some(tileset) = self.tilesets.get(layer.tileset) else {
                continue;
            };
            let source = tileset.tile_size();
            if source.w == 0 || source.h == 0 {
                continue;
            }
            // One square integer magnification per tileset: fill the grid tile
            // with a whole multiple of the source tile, floored so it stays
            // crisp. (draw_sprite_ex scales by a single factor; a grid tile is
            // meant to be a whole multiple of its source tile.)
            let scale = (self.grid.tile_size().w / source.w).max(1);
            for (index, cell) in layer.cells.iter().enumerate() {
                let Some(id) = cell else { continue };
                let Some(region) = tileset.region(*id) else {
                    continue;
                };
                let index = index as u32;
                let rect = self.grid.tile_rect(index % cols, index / cols);
                screen.draw_sprite_ex(
                    tileset.sheet(),
                    rect.origin,
                    BlitOptions {
                        region: Some(region),
                        scale,
                        ..BlitOptions::default()
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    const RED: Color = Color::rgb(255, 0, 0);
    const BLUE: Color = Color::rgb(0, 0, 255);

    /// A 2×1-pixel sheet of two 1×1 tiles: tile 0 red, tile 1 blue.
    fn two_color_sheet() -> Sprite {
        let mut sheet = Sprite::new(Size::new(2, 1));
        sheet.set(Point::new(0, 0), RED);
        sheet.set(Point::new(1, 0), BLUE);
        sheet
    }

    fn pixel(surface: &Surface, x: u32, y: u32) -> u32 {
        surface.as_slice()[(y * surface.size().w + x) as usize]
    }

    #[test]
    fn tile_ids_round_trip() {
        assert_eq!(TileId::new(7).get(), 7);
        assert_eq!(TileSetId::new(3).get(), 3);
    }

    #[test]
    fn tileset_grid_slices_regions_and_bounds_check() {
        let set = TileSet::grid(two_color_sheet(), Size::new(1, 1), 2);
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
        assert_eq!(
            set.region(TileId::new(0)),
            Some(Rect::new(Point::new(0, 0), Size::new(1, 1)))
        );
        assert_eq!(
            set.region(TileId::new(1)),
            Some(Rect::new(Point::new(1, 0), Size::new(1, 1)))
        );
        // Past the sheet: no region.
        assert_eq!(set.region(TileId::new(2)), None);
    }

    #[test]
    fn tileset_slices_a_multi_row_sheet() {
        // A 2×2 sheet of four 1×1 tiles over 2 columns: ids 0,1 on the top row,
        // 2,3 on the bottom.
        let set = TileSet::grid(Sprite::new(Size::new(2, 2)), Size::new(1, 1), 2);
        assert_eq!(set.len(), 4);
        assert_eq!(
            set.region(TileId::new(3)),
            Some(Rect::new(Point::new(1, 1), Size::new(1, 1)))
        );
    }

    #[test]
    fn tileset_flags_default_zero_and_read_back() {
        let set = TileSet::grid(two_color_sheet(), Size::new(1, 1), 2).with_flags(vec![9]);
        assert_eq!(set.flags(TileId::new(0)), 9);
        assert_eq!(set.flags(TileId::new(1)), 0, "unset ids read zero");
        let bare = TileSet::grid(two_color_sheet(), Size::new(1, 1), 2);
        assert_eq!(bare.flags(TileId::new(0)), 0, "no flags means all zero");
    }

    #[test]
    fn a_degenerate_tileset_is_empty_not_a_panic() {
        let set = TileSet::grid(Sprite::new(Size::new(4, 4)), Size::new(0, 0), 0);
        assert!(set.is_empty());
        assert_eq!(set.region(TileId::new(0)), None);
    }

    #[test]
    fn tilesets_resolve_by_id() {
        let sets = TileSets::new(vec![TileSet::grid(two_color_sheet(), Size::new(1, 1), 2)]);
        assert_eq!(sets.len(), 1);
        assert!(sets.get(TileSetId::new(0)).is_some());
        assert!(sets.get(TileSetId::new(1)).is_none());
    }

    #[test]
    fn tilemap_validates_dimensions_and_cell_counts() {
        let layer = TileLayer::new(TileSetId::new(0), vec![None; 6]);
        assert!(TileMap::new(Size::new(3, 2), vec![layer]).is_ok());

        assert_eq!(
            TileMap::new(Size::new(0, 2), vec![]).unwrap_err(),
            TileMapError::ZeroSize
        );

        let short = TileLayer::new(TileSetId::new(0), vec![None; 5]);
        assert_eq!(
            TileMap::new(Size::new(3, 2), vec![short]).unwrap_err(),
            TileMapError::CellCountMismatch {
                layer: 0,
                expected: 6,
                got: 5,
            }
        );
    }

    #[test]
    fn tilemap_round_trips_through_json() {
        let map = TileMap::new(
            Size::new(2, 1),
            vec![TileLayer::new(
                TileSetId::new(0),
                vec![Some(TileId::new(1)), None],
            )],
        )
        .expect("valid map");
        let json = serde_json::to_string(&map).expect("serialise");
        let back: TileMap = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, map);
        back.validate().expect("a round-tripped map stays valid");
    }

    #[test]
    fn tile_grid_lays_out_rects_and_centres_sprites() {
        let grid = TileGrid::new(Point::new(10, 20), Size::new(8, 8), Size::new(3, 2));
        assert_eq!(grid.pixel_size(), Size::new(24, 16));
        assert_eq!(
            grid.tile_rect(2, 1),
            Rect::new(Point::new(26, 28), Size::new(8, 8))
        );
        // A 4×4 sprite centres with a 2px inset on every side of the 8px tile.
        assert_eq!(
            grid.centered_in_tile(0, 0, Size::new(4, 4)),
            Point::new(12, 22)
        );
    }

    #[test]
    fn tile_grid_centres_within_an_area() {
        // A 2×2 tile grid of 10px tiles (20×20) centred in a 50×30 band that
        // starts 4px down: x offset (50-20)/2 = 15, y = 4 + (30-20)/2 = 9.
        let area = Rect::new(Point::new(0, 4), Size::new(50, 30));
        let grid = TileGrid::centered_in(area, Size::new(10, 10), Size::new(2, 2));
        assert_eq!(grid.origin(), Point::new(15, 9));
    }

    #[test]
    fn view_paints_cells_and_skips_empties() {
        // A 2×2 map, one layer, cells [red, empty, blue, red] at 1× scale.
        let sets = TileSets::new(vec![TileSet::grid(two_color_sheet(), Size::new(1, 1), 2)]);
        let map = TileMap::new(
            Size::new(2, 2),
            vec![TileLayer::new(
                TileSetId::new(0),
                vec![
                    Some(TileId::new(0)),
                    None,
                    Some(TileId::new(1)),
                    Some(TileId::new(0)),
                ],
            )],
        )
        .expect("valid map");
        let grid = TileGrid::new(Point::ORIGIN, Size::new(1, 1), Size::new(2, 2));
        let view = TileMapView::new(&map, &grid, &sets);

        let bg = Color::rgb(0, 0, 0);
        let mut screen = Surface::new(Size::new(2, 2), bg);
        view.render(&mut screen);

        assert_eq!(pixel(&screen, 0, 0), RED.packed());
        assert_eq!(
            pixel(&screen, 1, 0),
            bg.packed(),
            "the empty cell shows the backdrop"
        );
        assert_eq!(pixel(&screen, 0, 1), BLUE.packed());
        assert_eq!(pixel(&screen, 1, 1), RED.packed());
    }

    #[test]
    fn view_scales_each_source_tile_to_the_grid_tile() {
        // A 1×1 source tile drawn into a 3px grid tile fills a 3×3 block.
        let sets = TileSets::new(vec![TileSet::grid(two_color_sheet(), Size::new(1, 1), 2)]);
        let map = TileMap::new(
            Size::new(1, 1),
            vec![TileLayer::new(
                TileSetId::new(0),
                vec![Some(TileId::new(1))],
            )],
        )
        .expect("valid map");
        let grid = TileGrid::new(Point::ORIGIN, Size::new(3, 3), Size::new(1, 1));
        let view = TileMapView::new(&map, &grid, &sets);

        let mut screen = Surface::new(Size::new(3, 3), Color::rgb(0, 0, 0));
        view.render(&mut screen);
        for y in 0..3 {
            for x in 0..3 {
                assert_eq!(pixel(&screen, x, y), BLUE.packed(), "at ({x},{y})");
            }
        }
    }

    #[test]
    fn view_skips_an_unresolved_tileset() {
        // A layer pointing at tileset 5 (absent) draws nothing, no panic.
        let sets = TileSets::new(vec![TileSet::grid(two_color_sheet(), Size::new(1, 1), 2)]);
        let map = TileMap::new(
            Size::new(1, 1),
            vec![TileLayer::new(
                TileSetId::new(5),
                vec![Some(TileId::new(0))],
            )],
        )
        .expect("valid map");
        let grid = TileGrid::new(Point::ORIGIN, Size::new(1, 1), Size::new(1, 1));
        let view = TileMapView::new(&map, &grid, &sets);

        let bg = Color::rgb(7, 7, 7);
        let mut screen = Surface::new(Size::new(1, 1), bg);
        view.render(&mut screen);
        assert_eq!(pixel(&screen, 0, 0), bg.packed());
    }
}
