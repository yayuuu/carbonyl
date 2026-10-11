use crate::{gfx::{Point, Size}, input::Key, ui::navigation::NavigationAction};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, PartialEq, Debug)]
pub struct Control {
    pub id: i32,
    pub kind: i32,
    pub label: String,
    pub value: String,
    pub flags: u32,
}
#[derive(Default)]
pub struct Dialog {
    pub id: u64,
    pub title: String,
    pub controls: Vec<Control>,
    focus: usize,
    cursor: usize,
    selected: bool,
    utf8: Vec<u8>,
    scroll: usize,
}
fn clean(value: &str) -> String {
    value.chars().map(|c| if c.is_control() && c != '\n' { ' ' } else { c }).collect()
}
fn clipped(value: &str, width: usize) -> String {
    let mut text=String::new(); let mut used=0;
    for c in value.chars() {
        let n=c.width().unwrap_or(0);
        if used+n>width { break; }
        text.push(c); used+=n;
    }
    text
}
fn wrapped(value: &str, width: usize) -> Vec<String> {
    let mut result=Vec::new();
    for line in clean(value).split('\n') {
        let mut row=String::new(); let mut used=0;
        for c in line.chars() {
            let n=c.width().unwrap_or(0);
            if used+n>width && !row.is_empty() { result.push(row); row=String::new(); used=0; }
            if n<=width { row.push(c); used+=n; }
        }
        result.push(row);
    }
    result
}
impl Dialog {
    pub fn update(&mut self, id:u64, title:String, mut controls:Vec<Control>) {
        if id==0 { *self=Self::default(); return; }
        controls.push(Control{id:-1,kind:1,label:"Zamknij".into(),value:String::new(),flags:1});
        if id==self.id {
            let focused=self.controls.get(self.focus).map(|c| c.id);
            for control in &mut controls {
                if control.kind==3 {
                    if let Some(old)=self.controls.iter().find(|c| c.id==control.id) {
                        control.value=old.value.clone();
                    }
                }
            }
            self.controls=controls;
            self.focus=focused.and_then(|id|self.controls.iter().position(|c|c.id==id))
                .unwrap_or_else(||self.first_focus());
        } else {
            *self=Self{id,title:clean(&title),controls,..Self::default()};
            self.focus=self.controls.iter().position(|c|c.kind==3 && c.flags&1!=0)
                .or_else(||self.controls.iter().position(|c|c.kind>0 && c.flags&9==9))
                .unwrap_or_else(||self.first_focus());
            self.focus_field();
        }
    }
    fn first_focus(&self)->usize {
        self.controls.iter().position(|c|c.kind>0 && c.flags&1!=0).unwrap_or(0)
    }
    fn focus_field(&mut self) {
        self.cursor=self.controls.get(self.focus).map_or(0,|c|c.value.len());
        self.selected=true; self.utf8.clear();
    }
    fn action(&self, kind:i32)->NavigationAction {
        let control=&self.controls[self.focus];
        NavigationAction::DialogAction(self.id,control.id,
            if control.id==-1 {2} else {kind},control.value.clone())
    }
    fn next(&mut self, direction:i32) {
        for _ in 0..self.controls.len() {
            self.focus=(self.focus as i32+direction).rem_euclid(self.controls.len() as i32) as usize;
            let c=&self.controls[self.focus];
            if c.kind>0 && c.flags&1!=0 {self.focus_field();break;}
        }
    }
    pub fn keypress(&mut self,key:&Key)->NavigationAction {
        if key.char==0x1b { return NavigationAction::DialogAction(self.id,-1,2,String::new()); }
        if key.char==9 {self.next(if key.modifiers.shift {-1} else {1});return NavigationAction::Ignore;}
        if matches!(key.char,0x11|0x12) {self.next(if key.char==0x11 {-1} else {1});return NavigationAction::Ignore;}
        if key.char==13 {
            if self.controls[self.focus].kind==3 {
                if let Some(index)=self.controls.iter().position(|c|c.kind==1 && c.flags&17==17)
                    .or_else(||self.controls.iter().position(|c|c.kind==1 && c.flags&1!=0 && c.id!=-1)) {
                    self.focus=index;
                }
            }
            return self.action(0);
        }
        if self.controls[self.focus].kind!=3 {
            return if key.char==b' ' {self.action(0)} else {NavigationAction::Ignore};
        }
        let control=&mut self.controls[self.focus];
        self.cursor=self.cursor.min(control.value.len());
        match key.char {
            1=>{self.selected=true;return NavigationAction::Ignore;},
            0x14=>{
                self.cursor=if self.selected {0} else {control.value[..self.cursor].char_indices().last().map_or(0,|p|p.0)};
                self.selected=false;return NavigationAction::Ignore;
            },
            0x13=>{
                self.cursor=if self.selected {control.value.len()} else {self.cursor+control.value[self.cursor..].chars().next().map_or(0,char::len_utf8)};
                self.selected=false;return NavigationAction::Ignore;
            },
            8|127=>{
                if self.selected {control.value.clear();self.cursor=0;}
                else if self.cursor>0 {
                    let previous=control.value[..self.cursor].char_indices().last().unwrap().0;
                    control.value.replace_range(previous..self.cursor,"");self.cursor=previous;
                }
                self.selected=false;
            },
            32..=126|128..=255=>{
                self.utf8.push(key.char);
                let value=match std::str::from_utf8(&self.utf8) {
                    Ok(value)=>value.to_owned(),
                    Err(error) if error.error_len().is_none()=>return NavigationAction::Ignore,
                    Err(_)=>{self.utf8.clear();return NavigationAction::Ignore;}
                };
                self.utf8.clear();
                if self.selected {control.value.clear();self.cursor=0;self.selected=false;}
                control.value.insert_str(self.cursor,&value);self.cursor+=value.len();
            },
            _=>return NavigationAction::Ignore,
        }
        self.action(1)
    }
    // Frame coordinates are terminal cells, including the address row.
    pub fn bounds(&self,size:Size)->(usize,usize,usize,usize) {
        let w=(size.width as usize).saturating_sub(2).min(82).max(1);
        let h=(size.height as usize).saturating_sub(2).min(26).max(1);
        (((size.width as usize).saturating_sub(w))/2,
            (size.height as usize+1).saturating_sub(h)/2,w,h)
    }
    fn rows(&self,width:usize)->Vec<(String,Option<usize>)> {
        let mut rows=Vec::new();
        for (i,c) in self.controls.iter().enumerate() {
            let label=clean(&c.label);
            let text=match c.kind {
                1=>format!("[ {} ]{}",label,if c.flags&1==0 {" (nieaktywne)"} else {""}),
                2=>format!("[{}] {}",if c.flags&2==0 {' '} else {'x'},label),
                3=>{
                    let value=if c.flags&4!=0 {"*".repeat(c.value.chars().count())} else {clean(&c.value)};
                    format!("{}[ {} ]",if label.is_empty(){String::new()}else{format!("{}: ",label)},value)
                },
                _=>label,
            };
            for line in wrapped(&text,width.max(1)) {rows.push((line,if c.kind>0 {Some(i)} else {None}));}
        }
        rows
    }
    pub fn render(&mut self,size:Size)->Vec<(Point,String,bool)> {
        let (x,y,w,h)=self.bounds(size);
        let width=w.saturating_sub(4).max(1);
        let rows=self.rows(width);
        let visible=h.saturating_sub(4).max(1);
        if let Some(row)=rows.iter().position(|(_,index)|*index==Some(self.focus)) {
            if row<self.scroll {self.scroll=row;}
            if row>=self.scroll+visible {self.scroll=row+1-visible;}
        }
        self.scroll=self.scroll.min(rows.len().saturating_sub(visible));
        let mut output=Vec::new();
        if w<5 || h<5 {return output;}
        output.push((Point::new(x as i32,y as i32),format!("┌{}┐","─".repeat(w-2)),false));
        for row in 1..h-1 {
            let (text,focus)=if row==1 {(clipped(&format!("Carbonyl · {}",self.title),w-4),false)}
                else if row==h-2 {(clipped("Tab: wybór  Enter: zatwierdź  Esc: anuluj",w-4),false)}
                else {rows.get(self.scroll+row-2).map(|(text,index)|(text.clone(),*index==Some(self.focus))).unwrap_or_default()};
            let padded=format!("│ {}{} │",text," ".repeat(w.saturating_sub(4+text.width())));
            output.push((Point::new(x as i32,(y+row) as i32),padded,focus));
        }
        output.push((Point::new(x as i32,(y+h-1) as i32),format!("└{}┘","─".repeat(w-2)),false));
        output
    }
    pub fn mouse(&mut self,point:Point,size:Size,activate:bool)->NavigationAction {
        let (x,y,w,h)=self.bounds(size);
        if point.x>=x as i32 && point.x<(x+w) as i32 && point.y>=(y+2) as i32 && point.y<(y+h.saturating_sub(2)) as i32 {
            let row=self.scroll+point.y as usize-y-2;
            if let Some((_,Some(index)))=self.rows(w.saturating_sub(4).max(1)).get(row) {
                let index=*index;
                if self.controls[index].flags&1!=0 {
                    if self.focus!=index {self.focus=index;self.focus_field();}
                    if activate && self.controls[index].kind!=3 {return self.action(0);}
                }
            }
        }
        NavigationAction::Ignore
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field()->Control {Control{id:1,kind:3,label:"Nazwa".into(),value:"poprzednia".into(),flags:1}}
    fn button()->Control {Control{id:2,kind:1,label:"OK".into(),value:String::new(),flags:17}}
    #[test]
    fn utf8_replaces_selection_and_enter_activates_real_default_button() {
        let mut dialog=Dialog::default();dialog.update(7,"Test".into(),vec![field(),button()]);
        for byte in "żółć".bytes() {dialog.keypress(&byte.into());}
        assert_eq!(dialog.controls[0].value,"żółć");
        dialog.keypress(&127.into());assert_eq!(dialog.controls[0].value,"żół");
        dialog.update(7,"Test".into(),vec![field(),button()]);
        assert_eq!(dialog.controls[0].value,"żół","periodic snapshots must not overwrite a pending edit");
        assert!(matches!(dialog.keypress(&13.into()),NavigationAction::DialogAction(7,2,0,_)));
        assert!(matches!(dialog.keypress(&27.into()),NavigationAction::DialogAction(7,-1,2,_)));
    }
    #[test]
    fn frame_filters_terminal_controls_and_masks_passwords() {
        let mut f=field();f.flags|=4;f.value="secret".into();
        let mut dialog=Dialog::default();dialog.update(1,"Title\x1b[2J".into(),vec![f,button()]);
        let rendered=dialog.render(Size::new(80,25)).into_iter().map(|(_,text,_)|text).collect::<String>();
        assert!(rendered.contains("******"));assert!(!rendered.contains("secret"));assert!(!rendered.contains('\x1b'));
    }
}
