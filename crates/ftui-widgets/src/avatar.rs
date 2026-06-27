#![forbid(unsafe_code)]
//! Avatar widget with emoji, optional status dot, and label.
use crate::{Widget, clear_text_row, draw_text_span};
use ftui_core::geometry::Rect;
use ftui_core::terminal_capabilities::TerminalCapabilities;
use ftui_render::cell::PackedRgba;
use ftui_render::frame::Frame;
use ftui_style::Style;
use ftui_text::display_width;

/// Status of an agent, mapped to a colored dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentStatus {
    Spawned, Running, Thinking, Blocked, Failed, Completed,
}
impl AgentStatus {
    pub fn dot_color(self) -> PackedRgba {
        match self {
            Self::Spawned => PackedRgba::rgb(128,128,128),
            Self::Running => PackedRgba::rgb(0,200,0),
            Self::Thinking => PackedRgba::rgb(180,80,255),
            Self::Blocked => PackedRgba::rgb(255,200,0),
            Self::Failed => PackedRgba::rgb(255,50,50),
            Self::Completed => PackedRgba::rgb(50,130,255),
        }
    }
    pub fn as_label(self) -> &'static str {
        match self { Self::Spawned=>"spawned",Self::Running=>"running",Self::Thinking=>"thinking",Self::Blocked=>"blocked",Self::Failed=>"failed",Self::Completed=>"completed" }
    }
}

#[derive(Debug, Clone)]
pub struct Avatar<'a> {
    emoji: &'a str,
    status: Option<AgentStatus>,
    label: Option<&'a str>,
    focused: bool,
    style: Style,
    label_style: Style,
    fallback: Option<&'a str>,
}
impl<'a> Avatar<'a> {
    pub fn new(emoji: &'a str) -> Self { Self{emoji,status:None,label:None,focused:false,style:Style::default(),label_style:Style::default(),fallback:None} }
    pub fn with_status(mut self,s:AgentStatus) -> Self{self.status=Some(s);self}
    pub fn with_label(mut self,l:&'a str)->Self{self.label=Some(l);self}
    pub fn with_focused(mut self,f:bool)->Self{self.focused=f;self}
    pub fn with_style(mut self,s:Style)->Self{self.style=s;self}
    pub fn with_label_style(mut self,s:Style)->Self{self.label_style=s;self}
    pub fn with_fallback(mut self,f:&'a str)->Self{self.fallback=Some(f);self}
    pub fn emoji(&self)->&str{self.emoji}
    pub fn status(&self)->Option<AgentStatus>{self.status}
    pub fn label(&self)->Option<&str>{self.label}
    pub fn is_focused(&self)->bool{self.focused}
    pub fn fallback(&self)->Option<&str>{self.fallback}
    pub fn avatar_width(&self)->u16{display_width(self.emoji)as u16}
    pub fn width(&self)->u16{
        let mut w=self.avatar_width();
        if self.status.is_some(){w=w.saturating_add(1);}
        if let Some(l)=self.label{let lw=display_width(l)as u16;w=w.saturating_add(1).saturating_add(lw);}
        w
    }
    fn effective_emoji(&self)->&str{
        if self.fallback.is_some()&&!TerminalCapabilities::with_overrides().unicode_emoji{self.fallback.unwrap_or(self.emoji)}else{self.emoji}
    }
    fn dot_char(&self)->&'static str{if TerminalCapabilities::with_overrides().unicode_emoji{"●"}else{"o"}}
}

impl Widget for Avatar<'_> {
    fn render(&self,area:Rect,frame:&mut Frame){
        if area.is_empty(){return;}
        let deg=frame.buffer.degradation;
        if !deg.render_content(){clear_text_row(frame,area,Style::default());return;}
        let us=deg.apply_styling();
        let base=if us{self.style}else{Style::default()};
        let es=if self.focused{base.bold()}else{base};
        clear_text_row(frame,area,es);
        let y=area.y;let mx=area.right();let mut x=area.x;
        x=draw_text_span(frame,x,y,self.effective_emoji(),es,mx);
        if let Some(st)=self.status{
            if x<mx{
                let ds=if us{Style::new().fg(st.dot_color())}else{Style::default()};
                let ds=if self.focused{ds.bold()}else{ds};
                x=draw_text_span(frame,x,y,self.dot_char(),ds,mx);
            }
        }
        if let Some(l)=self.label{
            if x<mx{
                x+=1;
                let ls=if us{self.label_style}else{Style::default()};
                let ls=if self.focused{ls.bold()}else{ls};
                let _=draw_text_span(frame,x,y,l,ls,mx);
            }
        }
    }
    fn is_essential(&self)->bool{true}
}

impl ftui_a11y::Accessible for Avatar<'_> {
    fn accessibility_nodes(&self,area:Rect)->Vec<ftui_a11y::node::A11yNodeInfo>{
        use ftui_a11y::node::{A11yNodeInfo,A11yRole};
        let id=crate::a11y_node_id(area);
        let mut n=self.effective_emoji().to_owned();
        if let Some(l)=self.label{n.push(' ');n.push_str(l);}
        if let Some(st)=self.status{n.push_str(" (");n.push_str(st.as_label());n.push(')');}
        vec![A11yNodeInfo::new(id,A11yRole::Label,area).with_name(n)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_a11y::Accessible;
    use ftui_core::capability_override::{CapabilityOverride,with_capability_override};
    use ftui_render::budget::DegradationLevel;
    use ftui_render::frame::Frame;
    use ftui_render::grapheme_pool::GraphemePool;
    fn c(b:&ftui_render::buffer::Buffer,x:u16,y:u16)->Option<char>{b.get(x,y).and_then(|c|c.content.as_char())}
    fn r(b:&ftui_render::buffer::Buffer,y:u16,w:u16)->String{(0..w).map(|x|c(b,x,y).unwrap_or(' ')).collect()}
    #[test] fn new_avatar(){let a=Avatar::new("A");assert_eq!(a.emoji(),"A");assert!(a.status().is_none());assert!(a.label().is_none());assert!(!a.is_focused());assert!(a.fallback().is_none());}
    #[test] fn builder_status(){let a=Avatar::new("X").with_status(AgentStatus::Running);assert_eq!(a.status(),Some(AgentStatus::Running));}
    #[test] fn builder_label(){let a=Avatar::new("X").with_label("Bot");assert_eq!(a.label(),Some("Bot"));}
    #[test] fn builder_focused(){let a=Avatar::new("X").with_focused(true);assert!(a.is_focused());}
    #[test] fn builder_fallback(){let a=Avatar::new("X").with_fallback("[c]");assert_eq!(a.fallback(),Some("[c]"));}
    #[test] fn dot_colors(){use AgentStatus::*;let v=[Spawned,Running,Thinking,Blocked,Failed,Completed];for(i,a)in v.iter().enumerate(){let c=a.dot_color();assert_ne!(c,PackedRgba::TRANSPARENT);for(j,b)in v.iter().enumerate(){if i!=j{assert_ne!(c,b.dot_color());}}}}
    #[test] fn labels(){assert_eq!(AgentStatus::Spawned.as_label(),"spawned");assert_eq!(AgentStatus::Running.as_label(),"running");}
    #[test] fn width_bare(){assert_eq!(Avatar::new("A").width(),1);}
    #[test] fn width_dot(){assert_eq!(Avatar::new("A").with_status(AgentStatus::Running).width(),2);}
    #[test] fn width_label(){assert_eq!(Avatar::new("A").with_label("Hi").width(),4);}
    #[test] fn width_both(){assert_eq!(Avatar::new("A").with_status(AgentStatus::Running).with_label("G").width(),4);}
    #[test] fn render_simple(){let mut p=GraphemePool::new();let mut f=Frame::new(10,1,&mut p);Avatar::new("A").render(Rect::new(0,0,10,1),&mut f);assert_eq!(c(&f.buffer,0,0),Some('A'));}
    #[test] fn render_label(){let mut p=GraphemePool::new();let mut f=Frame::new(10,1,&mut p);Avatar::new("A").with_label("BC").render(Rect::new(0,0,10,1),&mut f);assert_eq!(r(&f.buffer,0,10),"A BC      ");}
    #[test] fn render_dot(){with_capability_override(CapabilityOverride::new().unicode_emoji(Some(true)),||{let mut p=GraphemePool::new();let mut f=Frame::new(10,1,&mut p);Avatar::new("A").with_status(AgentStatus::Running).render(Rect::new(0,0,10,1),&mut f);assert_eq!(c(&f.buffer,0,0),Some('A'));assert_eq!(c(&f.buffer,1,0),Some('\u{25CF}'));});}
    #[test] fn render_zero(){let mut p=GraphemePool::new();let mut f=Frame::new(10,1,&mut p);Avatar::new("A").render(Rect::new(0,0,0,0),&mut f);}
    #[test] fn render_skeleton(){let mut p=GraphemePool::new();let mut f=Frame::new(5,1,&mut p);Avatar::new("X").with_label("YY").render(Rect::new(0,0,5,1),&mut f);f.buffer.degradation=DegradationLevel::Skeleton;Avatar::new("X").with_label("YY").render(Rect::new(0,0,5,1),&mut f);assert_eq!(r(&f.buffer,0,5),"     ");}
    #[test] fn render_nostyle(){with_capability_override(CapabilityOverride::new().unicode_emoji(Some(true)),||{let fg=PackedRgba::rgb(12,34,56);let mut p=GraphemePool::new();let mut f=Frame::new(10,1,&mut p);f.buffer.degradation=DegradationLevel::NoStyling;Avatar::new("A").with_style(Style::new().fg(fg)).with_status(AgentStatus::Running).with_label("Hi").render(Rect::new(0,0,10,1),&mut f);assert_eq!(f.buffer.get(0,0).unwrap().fg,PackedRgba::WHITE);});}
    #[test] fn render_ascii(){with_capability_override(CapabilityOverride::new().unicode_emoji(Some(false)),||{let mut p=GraphemePool::new();let mut f=Frame::new(10,1,&mut p);Avatar::new("X").with_fallback("[c]").with_status(AgentStatus::Running).render(Rect::new(0,0,10,1),&mut f);assert_eq!(c(&f.buffer,0,0),Some('['));assert_eq!(c(&f.buffer,3,0),Some('o'));});}
    #[test] fn render_focused(){let mut p=GraphemePool::new();let mut f=Frame::new(5,1,&mut p);Avatar::new("A").with_focused(true).render(Rect::new(0,0,5,1),&mut f);assert!(f.buffer.get(0,0).unwrap().attrs.has_flag(ftui_render::cell::StyleFlags::BOLD));}
    #[test] fn essential(){assert!(Avatar::new("X").is_essential());}
    #[test] fn clear_stale(){let mut p=GraphemePool::new();let mut f=Frame::new(8,1,&mut p);Avatar::new("A").with_label("Long").render(Rect::new(0,0,8,1),&mut f);Avatar::new("A").with_label("Hi").render(Rect::new(0,0,8,1),&mut f);assert_eq!(r(&f.buffer,0,8),"A Hi    ");}
    #[test] fn a11y(){use ftui_a11y::node::A11yRole;let av=Avatar::new("X").with_label("B").with_status(AgentStatus::Spawned);let ns=av.accessibility_nodes(Rect::new(0,0,5,1));assert_eq!(ns.len(),1);assert_eq!(ns[0].role,A11yRole::Label);let n=ns[0].name.as_deref().unwrap_or("");assert!(n.contains("X")&&n.contains("B")&&n.contains("spawned"));}
    #[test] fn clone_debug(){let a=Avatar::new("A").with_label("T").with_status(AgentStatus::Failed);let b=a.clone();assert_eq!(a.emoji(),b.emoji());}
    #[test] fn dot_fallback(){with_capability_override(CapabilityOverride::new().unicode_emoji(Some(false)),||{let mut p=GraphemePool::new();let mut f=Frame::new(10,1,&mut p);Avatar::new("X").with_status(AgentStatus::Running).render(Rect::new(0,0,10,1),&mut f);assert_eq!(c(&f.buffer,0,0),Some('X'));assert_eq!(c(&f.buffer,1,0),Some('o'));});}
    #[test] fn truncate(){let mut p=GraphemePool::new();let mut f=Frame::new(3,1,&mut p);Avatar::new("ABCD").with_label("EF").render(Rect::new(0,0,3,1),&mut f);assert_eq!(r(&f.buffer,0,3),"ABC");}
}
