use super::*;
impl Utilities {
    pub(super) fn emoji_action(&mut self, a: Action, cx: &mut Context<Self>) {
        match a {
            Action::Select(i) => self.emoji.selected = i,
            Action::EmojiCategory(category) => {
                self.emoji.category = category;
                self.emoji.selected = 0;
            }
            Action::Activate | Action::Copy => {
                for e in self.emoji.activate(self.emoji.selected) {
                    match e {
                        model::EmojiEffect::PersistRecent(v) => {
                            cx.emit(UtilityEffect::RecentEmoji(v))
                        }
                        model::EmojiEffect::HideThenPaste(s) => {
                            cx.emit(UtilityEffect::HideThenPaste(s))
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
