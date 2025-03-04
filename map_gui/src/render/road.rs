use std::cell::RefCell;

use geom::{Distance, Polygon, Pt2D};
use map_model::{Building, LaneType, Map, Road, RoadID, NORMAL_LANE_THICKNESS};
use widgetry::{Color, Drawable, GeomBatch, GfxCtx, Line, Prerender, Text};

use crate::colors::ColorSchemeChoice;
use crate::options::CameraAngle;
use crate::render::{DrawOptions, Renderable};
use crate::{AppLike, ID};

pub struct DrawRoad {
    pub id: RoadID,
    zorder: isize,

    draw: RefCell<Option<Drawable>>,
}

impl DrawRoad {
    pub fn new(r: &Road) -> DrawRoad {
        DrawRoad {
            id: r.id,
            zorder: r.zorder,
            draw: RefCell::new(None),
        }
    }

    pub fn render<P: AsRef<Prerender>>(&self, prerender: &P, app: &dyn AppLike) -> GeomBatch {
        let prerender = prerender.as_ref();

        let mut batch = GeomBatch::new();
        let r = app.map().get_r(self.id);
        // TODO Need to detangle how road_center_line is used.
        let center_color = app.cs().road_center_line(r.get_rank());
        let color = if r.is_private() {
            center_color.lerp(app.cs().private_road, 0.5)
        } else {
            center_color
        };

        // Draw a center line every time two driving/bike/bus lanes of opposite direction are
        // adjacent.
        let mut width = Distance::ZERO;
        for pair in r.lanes_ltr().windows(2) {
            let ((l1, dir1, lt1), (_, dir2, lt2)) = (pair[0], pair[1]);
            width += app.map().get_l(l1).width;
            if dir1 != dir2 && lt1.is_for_moving_vehicles() && lt2.is_for_moving_vehicles() {
                let pl = r.get_left_side(app.map()).must_shift_right(width);
                batch.extend(
                    color,
                    pl.dashed_lines(
                        Distance::meters(0.25),
                        Distance::meters(2.0),
                        Distance::meters(1.0),
                    ),
                );
            }
        }

        // Draw the label
        if !r.is_light_rail() {
            let name = r.get_name(app.opts().language.as_ref());
            if r.center_pts.length() >= Distance::meters(30.0) && name != "???" {
                // TODO If it's definitely straddling bus/bike lanes, change the color? Or
                // even easier, just skip the center lines?
                let center_color = app.cs().road_center_line(r.get_rank());
                let fg = if r.is_private() {
                    center_color.lerp(app.cs().private_road, 0.5)
                } else {
                    center_color
                };
                let bg = if r.is_private() {
                    app.cs()
                        .zoomed_road_surface(LaneType::Driving, r.get_rank())
                        .lerp(app.cs().private_road, 0.5)
                } else {
                    app.cs()
                        .zoomed_road_surface(LaneType::Driving, r.get_rank())
                };

                if false {
                    // TODO Not ready yet
                    batch.append(
                        Line(name)
                            .fg(fg)
                            .render_curvey(prerender, &r.center_pts, 0.1),
                    );
                } else {
                    let txt = Text::from(Line(name).fg(fg)).bg(bg);
                    let (pt, angle) = r.center_pts.must_dist_along(r.center_pts.length() / 2.0);
                    batch.append(
                        txt.render_autocropped(prerender)
                            .scale(0.1)
                            .centered_on(pt)
                            .rotate(angle.reorient()),
                    );
                }
            }
        }

        // Driveways of connected buildings. These are grouped by road to limit what has to be
        // recalculated when road edits cause buildings to re-snap.
        for b in app.map().road_to_buildings(self.id) {
            draw_building_driveway(app, app.map().get_b(*b), &mut batch);
        }

        batch
    }

    pub fn clear_rendering(&mut self) {
        *self.draw.borrow_mut() = None;
    }
}

impl Renderable for DrawRoad {
    fn get_id(&self) -> ID {
        ID::Road(self.id)
    }

    fn draw(&self, g: &mut GfxCtx, app: &dyn AppLike, _: &DrawOptions) {
        let mut draw = self.draw.borrow_mut();
        if draw.is_none() {
            *draw = Some(g.upload(self.render(g, app)));
        }
        g.redraw(draw.as_ref().unwrap());
    }

    fn get_outline(&self, map: &Map) -> Polygon {
        // Highlight the entire thing, not just an outline
        map.get_r(self.id).get_thick_polygon(map)
    }

    fn contains_pt(&self, pt: Pt2D, map: &Map) -> bool {
        map.get_r(self.id).get_thick_polygon(map).contains_pt(pt)
    }

    fn get_zorder(&self) -> isize {
        self.zorder
    }
}

fn draw_building_driveway(app: &dyn AppLike, bldg: &Building, batch: &mut GeomBatch) {
    if app.opts().camera_angle == CameraAngle::Abstract || !app.opts().show_building_driveways {
        return;
    }

    // Trim the driveway away from the sidewalk's center line, so that it doesn't overlap.  For
    // now, this cleanup is visual; it doesn't belong in the map_model layer.
    let orig_pl = &bldg.driveway_geom;
    let driveway = orig_pl
        .slice(
            Distance::ZERO,
            orig_pl.length() - app.map().get_l(bldg.sidewalk()).width / 2.0,
        )
        .map(|(pl, _)| pl)
        .unwrap_or_else(|_| orig_pl.clone());
    if driveway.length() > Distance::meters(0.1) {
        batch.push(
            if app.opts().color_scheme == ColorSchemeChoice::NightMode {
                Color::hex("#4B4B4B")
            } else {
                app.cs().zoomed_road_surface(
                    LaneType::Sidewalk,
                    app.map().get_parent(bldg.sidewalk()).get_rank(),
                )
            },
            driveway.make_polygons(NORMAL_LANE_THICKNESS),
        );
    }
}
