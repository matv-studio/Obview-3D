//! Comportement des commandes sans GPU : saisie, menus, fenêtres modales, défilement.

use obv_ui::{
    FrameInput, Id, Key, Layer, Modifiers, Node, Size, Theme, ThemeKind, Ui, Vec2,
    controls::{self, TextInput},
    vec2,
};

struct Driver {
    ui: Ui,
    t: f64,
}

impl Driver {
    fn new() -> Self {
        Driver { ui: Ui::new(Theme::new(ThemeKind::Dark)), t: 0.0 }
    }
    fn frame(&mut self, input: FrameInput, build: &mut dyn FnMut(&mut Ui) -> Node) {
        self.t += 0.05;
        self.ui.begin_frame(FrameInput { size: vec2(800.0, 600.0), scale: 1.0, time: self.t, ..input });
        let mut root = build(&mut self.ui);
        self.ui.finish(&mut root);
    }
    fn idle(&mut self, build: &mut dyn FnMut(&mut Ui) -> Node) {
        self.frame(FrameInput::default(), build);
    }
    fn click(&mut self, at: Vec2, build: &mut dyn FnMut(&mut Ui) -> Node) {
        self.frame(FrameInput { pointer: Some(at), ..Default::default() }, build);
        self.frame(FrameInput { pointer: Some(at), buttons: [true, false, false], ..Default::default() }, build);
        self.frame(FrameInput { pointer: Some(at), ..Default::default() }, build);
    }
    fn keys(&mut self, keys: Vec<Key>, text: &str, mods: Modifiers, build: &mut dyn FnMut(&mut Ui) -> Node) {
        self.frame(FrameInput { keys, text: text.into(), modifiers: mods, ..Default::default() }, build);
    }
}

#[test]
fn saisie_de_texte() {
    let id = Id::new("champ");
    let mut text = String::from("abc");
    let mut d = Driver::new();
    let mut build = |ui: &mut Ui| {
        let cfg = TextInput { id, placeholder: "", width: Size::Px(200.0), icon: None, hint: None, clearable: false };
        Node::column().child(controls::text_input(ui, cfg, &mut text).0)
    };
    d.idle(&mut build);
    let r = d.ui.rect_of(id).unwrap();
    // Clic à droite du texte : curseur à la fin.
    d.click(vec2(r.max.x - 20.0, r.center().y), &mut build);
    assert!(d.ui.has_focus(id));
    d.keys(vec![], "d", Modifiers::NONE, &mut build);
    d.keys(vec![Key::Left, Key::Backspace], "", Modifiers::NONE, &mut build);
    d.keys(vec![Key::Home], "X", Modifiers::NONE, &mut build);
    // Ctrl+A puis frappe : remplace tout.
    let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
    d.keys(vec![Key::Char('a')], "", ctrl, &mut build);
    d.keys(vec![], "z", Modifiers::NONE, &mut build);
    d.keys(vec![Key::Enter], "", Modifiers::NONE, &mut build);
    let _ = &mut build;
    assert_eq!(text, "z");
}

#[test]
fn saisie_etapes() {
    let id = Id::new("champ");
    let mut text = String::from("abc");
    let mut d = Driver::new();
    let mut log = Vec::new();
    {
        let mut build = |ui: &mut Ui| {
            let cfg =
                TextInput { id, placeholder: "", width: Size::Px(200.0), icon: None, hint: None, clearable: false };
            let n = controls::text_input(ui, cfg, &mut text).0;
            log.push(text.clone());
            Node::column().child(n)
        };
        d.idle(&mut build);
        let r = d.ui.rect_of(id).unwrap();
        d.click(vec2(r.max.x - 20.0, r.center().y), &mut build);
        d.keys(vec![], "d", Modifiers::NONE, &mut build);
        d.keys(vec![Key::Left, Key::Backspace], "", Modifiers::NONE, &mut build);
        d.keys(vec![Key::Home], "X", Modifiers::NONE, &mut build);
    }
    assert!(log.contains(&"abcd".to_string()), "{log:?}");
    assert!(log.contains(&"abd".to_string()), "{log:?}");
    assert_eq!(text, "Xabd");
}

#[test]
fn menu_ferme_au_clic_exterieur_sans_cliquer_dessous() {
    let sel = Id::new("liste");
    let other = Id::new("autre");
    let mut chosen = None;
    let mut other_clicks = 0;
    let mut d = Driver::new();
    let mut build = |ui: &mut Ui| {
        if ui.clicked(other) {
            other_clicks += 1;
        }
        let (n, c) = controls::select(ui, sel, &["Un", "Deux", "Trois"], 0, 150.0);
        if c.is_some() {
            chosen = c;
        }
        Node::column().gap(200.0).child(n).child(Node::new().id(other).size(Size::Px(100.0), Size::Px(40.0)))
    };
    d.idle(&mut build);
    let r = d.ui.rect_of(sel).unwrap();
    d.click(r.center(), &mut build);
    assert!(d.ui.popup_open(sel));
    // Clic sur « autre » : ferme le menu et n'active pas l'élément dessous.
    let o = d.ui.rect_of(other).unwrap();
    d.click(o.center(), &mut build);
    assert!(!d.ui.popup_open(sel));
    // Réouverture, puis choix du troisième élément.
    d.click(r.center(), &mut build);
    d.idle(&mut build);
    let item = d.ui.rect_of(sel.with(2)).unwrap();
    d.click(item.center(), &mut build);
    let _ = &mut build;
    assert_eq!(other_clicks, 0);
    assert_eq!(chosen, Some(2));
}

#[test]
fn modale_bloque_le_dessous() {
    let below = Id::new("dessous");
    let mut clicks = 0;
    let mut d = Driver::new();
    let mut build = |ui: &mut Ui| {
        if ui.clicked(below) {
            clicks += 1;
        }
        let (m, _) = controls::modal(ui, Id::new("fenêtre"), "Titre", (300.0, 200.0), Node::new(), vec![]);
        ui.overlay(m, Layer::Modal);
        Node::column().child(Node::new().id(below).size(Size::Px(100.0), Size::Px(40.0)))
    };
    d.idle(&mut build);
    d.click(vec2(50.0, 20.0), &mut build);
    let _ = &mut build;
    assert_eq!(clicks, 0);
}

#[test]
fn molette_fait_defiler() {
    let sid = Id::new("zone");
    let mut d = Driver::new();
    let mut build = |_: &mut Ui| {
        let mut list = Node::scroll(sid).size(Size::Px(200.0), Size::Px(300.0));
        for _ in 0..100 {
            list.push(Node::new().height(Size::Px(20.0)));
        }
        Node::column().child(list)
    };
    d.idle(&mut build);
    for _ in 0..30 {
        d.frame(
            FrameInput { pointer: Some(vec2(50.0, 50.0)), scroll: vec2(0.0, -120.0), ..Default::default() },
            &mut build,
        );
    }
    for _ in 0..30 {
        d.idle(&mut build);
    }
    let st = d.ui.scrolls[&sid];
    assert_eq!(st.content, 2000.0);
    assert_eq!(st.offset, st.max(), "défilement borné au bas de la liste");
}
