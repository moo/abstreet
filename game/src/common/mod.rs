use std::collections::BTreeSet;

use geom::{Duration, Polygon, Time};
use map_gui::ID;
use map_model::{IntersectionID, Map, RoadID};
use sim::{AgentType, TripMode, TripPhaseType};
use widgetry::{
    lctrl, Color, EventCtx, GeomBatch, GfxCtx, HorizontalAlignment, Key, Line, Panel, ScreenDims,
    ScreenPt, ScreenRectangle, Text, TextSpan, Toggle, VerticalAlignment, Widget,
};

pub use self::select::RoadSelector;
pub use self::warp::{warp_to_id, Warping};
use crate::app::App;
use crate::app::Transition;
use crate::info::{ContextualActions, InfoPanel, Tab};
use crate::sandbox::TimeWarpScreen;

mod select;
mod warp;

// TODO This is now just used in two modes...
pub struct CommonState {
    // TODO Better to express these as mutex
    info_panel: Option<InfoPanel>,
    // Just for drawing the OSD
    cached_actions: Vec<Key>,
}

impl CommonState {
    pub fn new() -> CommonState {
        CommonState {
            info_panel: None,
            cached_actions: Vec::new(),
        }
    }

    pub fn event(
        &mut self,
        ctx: &mut EventCtx,
        app: &mut App,
        ctx_actions: &mut dyn ContextualActions,
    ) -> Option<Transition> {
        if let Some(t) = CommonState::debug_actions(ctx, app) {
            return Some(t);
        }

        // Layers can be launched from many places, many of which don't have a way of getting at
        // CommonState, which is only in sandbox and debug mode. It suffices to detect here if a
        // layer is open and close the info panel, luckily.
        if app.primary.layer.is_some() {
            self.info_panel = None;
        }

        if let Some(id) = app.primary.current_selection.clone() {
            // TODO Also have a hotkey binding for this?
            if app.per_obj.left_click(ctx, "show info") {
                app.primary.layer = None;
                self.info_panel =
                    Some(InfoPanel::new(ctx, app, Tab::from_id(app, id), ctx_actions));
                return None;
            }
        }

        if let Some(ref mut info) = self.info_panel {
            let (closed, maybe_t) = info.event(ctx, app, ctx_actions);
            if closed {
                self.info_panel = None;
            }
            if let Some(t) = maybe_t {
                return Some(t);
            }
        }

        if self.info_panel.is_none() {
            self.cached_actions.clear();
            if let Some(id) = app.primary.current_selection.clone() {
                // Allow hotkeys to work without opening the panel.
                for (k, action) in ctx_actions.actions(app, id.clone()) {
                    if ctx.input.pressed(k) {
                        return Some(ctx_actions.execute(ctx, app, id, action, &mut false));
                    }
                    self.cached_actions.push(k);
                }
            }
        }

        None
    }

    pub fn draw(&self, g: &mut GfxCtx, app: &App) {
        let keys = if let Some(ref info) = self.info_panel {
            info.draw(g, app);
            info.active_keys()
        } else {
            &self.cached_actions
        };
        let mut osd = if let Some(ref id) = app.primary.current_selection {
            CommonState::osd_for(app, id.clone())
        } else if app.opts.dev {
            Text::from_all(vec![
                Line("Nothing selected. Hint: "),
                Line("Ctrl+J").fg(g.style().text_hotkey_color),
                Line(" to warp"),
            ])
        } else {
            Text::new()
        };
        if !keys.is_empty() {
            osd.append(Line("   Hotkeys: "));
            for (idx, key) in keys.iter().enumerate() {
                if idx != 0 {
                    osd.append(Line(", "));
                }
                osd.append(Line(key.describe()).fg(g.style().text_hotkey_color));
            }
        }

        CommonState::draw_custom_osd(g, app, osd);
    }

    fn osd_for(app: &App, id: ID) -> Text {
        let map = &app.primary.map;
        let mut osd = Text::new();
        match id {
            ID::Lane(l) => {
                if app.opts.dev {
                    osd.append(Line(l.to_string()).bold_body());
                    osd.append(Line(" is "));
                }
                let r = map.get_parent(l);
                osd.append_all(vec![
                    Line(format!("{} of ", map.get_l(l).lane_type.describe())),
                    Line(r.get_name(app.opts.language.as_ref())).underlined(),
                ]);
                if app.opts.dev {
                    osd.append(Line(" ("));
                    osd.append(Line(r.id.to_string()).bold_body());
                    osd.append(Line(")"));
                }
            }
            ID::Building(b) => {
                if app.opts.dev {
                    osd.append(Line(b.to_string()).bold_body());
                    osd.append(Line(" is "));
                }
                let bldg = map.get_b(b);
                osd.append(Line(&bldg.address).underlined())
            }
            ID::ParkingLot(pl) => {
                osd.append(Line(pl.to_string()).bold_body());
            }
            ID::Intersection(i) => {
                if map.get_i(i).is_border() {
                    osd.append(Line("Border "));
                }

                if app.opts.dev {
                    osd.append(Line(i.to_string()).bold_body());
                } else {
                    osd.append(Line("Intersection"));
                }
                osd.append(Line(" of "));

                let mut road_names = BTreeSet::new();
                for r in &map.get_i(i).roads {
                    road_names.insert(map.get_r(*r).get_name(app.opts.language.as_ref()));
                }
                list_names(&mut osd, |l| l.underlined(), road_names);
            }
            ID::Car(c) => {
                if app.opts.dev {
                    osd.append(Line(c.to_string()).bold_body());
                } else {
                    osd.append(Line(format!("a {}", c.vehicle_type)));
                }
                if let Some(r) = app.primary.sim.bus_route_id(c) {
                    osd.append_all(vec![
                        Line(" serving "),
                        Line(&map.get_br(r).full_name).underlined(),
                    ]);
                }
            }
            ID::Pedestrian(p) => {
                if app.opts.dev {
                    osd.append(Line(p.to_string()).bold_body());
                } else {
                    osd.append(Line("a pedestrian"));
                }
            }
            ID::PedCrowd(list) => {
                osd.append(Line(format!("a crowd of {} pedestrians", list.len())));
            }
            ID::BusStop(bs) => {
                if app.opts.dev {
                    osd.append(Line(bs.to_string()).bold_body());
                } else {
                    osd.append(Line("transit stop "));
                    osd.append(Line(&map.get_bs(bs).name).underlined());
                }
                osd.append(Line(" served by "));

                let routes: BTreeSet<String> = map
                    .get_routes_serving_stop(bs)
                    .into_iter()
                    .map(|r| r.short_name.clone())
                    .collect();
                list_names(&mut osd, |l| l.underlined(), routes);
            }
            ID::Area(a) => {
                // Only selectable in dev mode anyway
                osd.append(Line(a.to_string()).bold_body());
            }
            ID::Road(r) => {
                if app.opts.dev {
                    osd.append(Line(r.to_string()).bold_body());
                    osd.append(Line(" is "));
                }
                osd.append(Line(map.get_r(r).get_name(app.opts.language.as_ref())).underlined());
            }
        }
        osd
    }

    pub fn draw_osd(g: &mut GfxCtx, app: &App) {
        let osd = if let Some(ref id) = app.primary.current_selection {
            CommonState::osd_for(app, id.clone())
        } else if app.opts.dev {
            Text::from_all(vec![
                Line("Nothing selected. Hint: "),
                Line("Ctrl+J").fg(g.style().text_hotkey_color),
                Line(" to warp"),
            ])
        } else {
            Text::new()
        };
        CommonState::draw_custom_osd(g, app, osd);
    }

    pub fn draw_custom_osd(g: &mut GfxCtx, app: &App, mut osd: Text) {
        if let Some(ref action) = app.per_obj.click_action {
            osd.append_all(vec![
                Line("; "),
                Line("click").fg(g.style().text_hotkey_color),
                Line(format!(" to {}", action)),
            ]);
        }

        // TODO Rendering the OSD is actually a bit hacky.

        // First the constant background
        let mut batch = GeomBatch::from(vec![(
            if app.primary.is_secondary {
                Color::BLUE
            } else {
                app.cs.panel_bg
            },
            Polygon::rectangle(g.canvas.window_width, 1.5 * g.default_line_height()),
        )]);
        batch.append(
            osd.render(g)
                .translate(10.0, 0.25 * g.default_line_height()),
        );

        if app.opts.dev && !g.is_screencap() {
            let dev_batch = Text::from("DEV").bg(Color::RED).render(g);
            let dims = dev_batch.get_dims();
            batch.append(dev_batch.translate(
                g.canvas.window_width - dims.width - 10.0,
                0.25 * g.default_line_height(),
            ));
        }
        let draw = g.upload(batch);
        let top_left = ScreenPt::new(0.0, g.canvas.window_height - 1.5 * g.default_line_height());
        g.redraw_at(top_left, &draw);
        g.canvas.mark_covered_area(ScreenRectangle::top_left(
            top_left,
            ScreenDims::new(g.canvas.window_width, 1.5 * g.default_line_height()),
        ));
    }

    // Meant to be used for launching from other states
    pub fn launch_info_panel(
        &mut self,
        ctx: &mut EventCtx,
        app: &mut App,
        tab: Tab,
        ctx_actions: &mut dyn ContextualActions,
    ) {
        app.primary.layer = None;
        self.info_panel = Some(InfoPanel::new(ctx, app, tab, ctx_actions));
    }

    pub fn info_panel_open(&self, app: &App) -> Option<ID> {
        self.info_panel.as_ref().and_then(|i| i.active_id(app))
    }

    /// Allow toggling of dev mode and warping to an object by ID.
    pub fn debug_actions(ctx: &mut EventCtx, app: &mut App) -> Option<Transition> {
        if ctx.input.pressed(lctrl(Key::S)) {
            app.opts.dev = !app.opts.dev;
        }
        if ctx.input.pressed(lctrl(Key::J)) {
            return Some(Transition::Push(warp::DebugWarp::new_state(ctx)));
        }
        if app.secondary.is_some() && ctx.input.pressed(lctrl(Key::Tab)) {
            // This swaps the primary and secondary PerMaps. Depending on what state the rest of
            // the app is in, things like IDs might totally change! This is very much a debug tool.
            let secondary = app.secondary.take().unwrap();
            let primary = std::mem::replace(&mut app.primary, secondary);
            app.secondary = Some(primary);
        }
        None
    }
}

// TODO Kinda misnomer
pub fn tool_panel(ctx: &mut EventCtx) -> Panel {
    Panel::new_builder(Widget::row(vec![
        ctx.style()
            .btn_plain
            .icon("system/assets/tools/home.svg")
            .hotkey(Key::Escape)
            .build_widget(ctx, "back"),
        ctx.style()
            .btn_plain
            .icon("system/assets/tools/settings.svg")
            .build_widget(ctx, "settings"),
    ]))
    .aligned(HorizontalAlignment::Left, VerticalAlignment::BottomAboveOSD)
    .build(ctx)
}

pub fn list_names<F: Fn(TextSpan) -> TextSpan>(txt: &mut Text, styler: F, names: BTreeSet<String>) {
    let len = names.len();
    for (idx, n) in names.into_iter().enumerate() {
        if idx != 0 {
            if idx == len - 1 {
                if len == 2 {
                    txt.append(Line(" and "));
                } else {
                    txt.append(Line(", and "));
                }
            } else {
                txt.append(Line(", "));
            }
        }
        txt.append(styler(Line(n)));
    }
}

// Shorter is better
pub fn cmp_duration_shorter(app: &App, after: Duration, before: Duration) -> Vec<TextSpan> {
    if after.epsilon_eq(before) {
        vec![Line("same")]
    } else if after < before {
        vec![
            Line((before - after).to_string(&app.opts.units)).fg(Color::GREEN),
            Line(" faster"),
        ]
    } else if after > before {
        vec![
            Line((after - before).to_string(&app.opts.units)).fg(Color::RED),
            Line(" slower"),
        ]
    } else {
        unreachable!()
    }
}

pub fn color_for_mode(app: &App, m: TripMode) -> Color {
    match m {
        TripMode::Walk => app.cs.unzoomed_pedestrian,
        TripMode::Bike => app.cs.unzoomed_bike,
        TripMode::Transit => app.cs.unzoomed_bus,
        TripMode::Drive => app.cs.unzoomed_car,
    }
}

pub fn color_for_agent_type(app: &App, a: AgentType) -> Color {
    match a {
        AgentType::Pedestrian => app.cs.unzoomed_pedestrian,
        AgentType::Bike => app.cs.unzoomed_bike,
        AgentType::Bus | AgentType::Train => app.cs.unzoomed_bus,
        AgentType::TransitRider => app.cs.bus_trip,
        AgentType::Car => app.cs.unzoomed_car,
    }
}

pub fn color_for_trip_phase(app: &App, tpt: TripPhaseType) -> Color {
    match tpt {
        TripPhaseType::Driving => app.cs.unzoomed_car,
        TripPhaseType::Walking => app.cs.unzoomed_pedestrian,
        TripPhaseType::Biking => app.cs.bike_trip,
        TripPhaseType::Parking => app.cs.parking_trip,
        TripPhaseType::WaitingForBus(_, _) => app.cs.bus_layer,
        TripPhaseType::RidingBus(_, _, _) => app.cs.bus_trip,
        TripPhaseType::Cancelled | TripPhaseType::Finished => unreachable!(),
        TripPhaseType::DelayedStart => Color::YELLOW,
    }
}

pub fn intersections_from_roads(roads: &BTreeSet<RoadID>, map: &Map) -> BTreeSet<IntersectionID> {
    let mut results = BTreeSet::new();
    for r in roads {
        let r = map.get_r(*r);
        for i in [r.src_i, r.dst_i] {
            if results.contains(&i) {
                continue;
            }
            if map.get_i(i).roads.iter().all(|r| roads.contains(r)) {
                results.insert(i);
            }
        }
    }
    results
}

pub fn checkbox_per_mode(
    ctx: &mut EventCtx,
    app: &App,
    current_state: &BTreeSet<TripMode>,
) -> Widget {
    let mut filters = Vec::new();
    for m in TripMode::all() {
        filters.push(
            Toggle::colored_checkbox(
                ctx,
                m.ongoing_verb(),
                color_for_mode(app, m),
                current_state.contains(&m),
            )
            .margin_right(24),
        );
    }
    Widget::custom_row(filters)
}

/// If you want a simulation to start after midnight, pass the result of this to
/// `SandboxMode::async_new`. It's less visually overwhelming and more performant to continue with a
/// loading screen than launching the jump-to-time UI. After a deadline of 0.5s, we will switch
/// over to the jump-to-time UI so the user can cancel.
pub fn jump_to_time_upon_startup(
    dt: Duration,
) -> Box<dyn FnOnce(&mut EventCtx, &mut App) -> Vec<Transition>> {
    Box::new(move |ctx, app| {
        ctx.loading_screen(format!("jump forward {}", dt), |ctx, _| {
            let deadline = Duration::seconds(0.5);
            app.primary
                .sim
                .time_limited_step(&app.primary.map, dt, deadline, &mut None);
            let target = Time::START_OF_DAY + dt;
            if app.primary.sim.time() != target {
                vec![Transition::Push(TimeWarpScreen::new_state(
                    ctx, app, target, None,
                ))]
            } else {
                vec![Transition::Keep]
            }
        })
    })
}
