// Execute the actual small theme helper against recording adapters. No GPUI,
// window, profile or product executable is opened by this private Rust fixture.
import test from 'node:test'
import assert from 'node:assert/strict'
import {readFileSync,mkdtempSync,writeFileSync,rmSync} from 'node:fs'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
import {spawnSync} from 'node:child_process'

test('stored themes update tokens and native appearance together; cold start retains preview override guard',()=>{
  const app=readFileSync(new URL('../../apps/tezbar/src/app.rs',import.meta.url),'utf8')
  const begin=app.indexOf('fn apply_theme('), end=app.indexOf('\nstruct RuntimeOwner',begin)
  assert(begin>=0 && end>begin)
  const helper=app.slice(begin,end)
  const cold=app.slice(app.indexOf('if preview.theme == "system"'),app.indexOf('        let tray =',app.indexOf('if preview.theme == "system"')))
  assert.match(cold,/view\.state\.theme/);assert.match(cold,/apply_theme\(theme, cx\)/)
  const root=mkdtempSync(join(tmpdir(),'tezbar-theme-helper-'))
  try {
    const file=join(root,'theme.rs'), binary=join(root,'theme-tests')
    writeFileSync(file,`
#[derive(Debug,PartialEq,Eq,Clone,Copy)] enum WindowAppearance { Light, Dark }
mod tezbar_ui_model { pub mod ui_state { #[derive(Clone,Copy)] pub enum Theme { White, Dark, System } } }
mod tezbar_ui { pub mod tokens {
    pub static VALUE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    pub fn set(value: &str)->Result<(),String>{*VALUE.lock().unwrap()=value.into();Ok(())}
} }
#[derive(Default)] struct App { appearance: Option<WindowAppearance>, refreshes: usize }
impl App {
    fn set_window_appearance(&mut self,value:Option<WindowAppearance>){self.appearance=value;}
    fn refresh_windows(&mut self){self.refreshes+=1;}
}
${helper}
#[test] fn stored_theme_applies_both_outputs_and_resets_system_override(){
    use tezbar_ui_model::ui_state::Theme;
    let mut app=App::default();
    for (theme,tokens,appearance) in [(Theme::White,"white",Some(WindowAppearance::Light)),(Theme::Dark,"dark",Some(WindowAppearance::Dark)),(Theme::System,"system",None)] {
        apply_theme(theme,&mut app);
        assert_eq!(*tezbar_ui::tokens::VALUE.lock().unwrap(),tokens);
        assert_eq!(app.appearance,appearance);
    }
    assert_eq!(app.refreshes,3);
}`)
    const compile=spawnSync('rustc',['+1.98.1','--edition=2024','--test',file,'-o',binary],{encoding:'utf8'})
    assert.equal(compile.status,0,compile.stderr)
    const run=spawnSync(binary,[],{encoding:'utf8'})
    assert.equal(run.status,0,run.stdout+run.stderr);assert.match(run.stdout,/1 passed/)
  } finally {rmSync(root,{recursive:true,force:true})}
})
