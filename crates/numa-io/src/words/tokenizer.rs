use std::collections::HashMap;
use std::path::Path;

pub const LENGTH: usize = 64;
const PAD: i64 = 0;
const EOS: i64 = 1;

pub struct Tokenizer {

    chars: HashMap<char, u32>,

    bytes: Vec<u32>,

    merges: HashMap<(u32, u32), (u32, u32)>,

    added: Vec<(String, u32)>,
}

#[derive(serde::Deserialize)]
struct File {
    model: Model,
    #[serde(default)]
    added_tokens: Vec<Added>,
}

#[derive(serde::Deserialize)]
struct Added {
    id: u32,
    content: String,
}

#[derive(serde::Deserialize)]
struct Model {
    vocab: HashMap<String, u32>,
    merges: Vec<(String, String)>,
}

impl Tokenizer {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
        Self::from_json(&text).map_err(|err| format!("{}: {err}", path.display()))
    }

    fn from_json(text: &[u8]) -> Result<Self, String> {
        let File { model, added_tokens } = serde_json::from_slice(text).map_err(|err| err.to_string())?;
        let id = |token: &str| model.vocab.get(token).copied();
        let unknown = id("<unk>").ok_or("no <unk> in the vocabulary")?;
        let bytes = (0..=255u8).map(|byte| id(&format!("<0x{byte:02X}>")).unwrap_or(unknown)).collect();
        let chars = model
            .vocab
            .iter()
            .filter_map(|(token, id)| {
                let mut chars = token.chars();
                match (chars.next(), chars.next()) {
                    (Some(only), None) => Some((only, *id)),
                    _ => None,
                }
            })
            .collect();
        let merges = model
            .merges
            .iter()
            .enumerate()
            .filter_map(|(rank, (a, b))| Some(((id(a)?, id(b)?), (rank as u32, id(&format!("{a}{b}"))?))))
            .collect();
        let mut added: Vec<(String, u32)> = added_tokens.into_iter().map(|token| (token.content, token.id)).collect();
        added.sort_by_key(|(content, _)| std::cmp::Reverse(content.len()));
        Ok(Self { chars, bytes, merges, added })
    }

    pub fn encode(&self, text: &str) -> Vec<i64> {
        let text = text.to_lowercase();
        let mut tokens: Vec<u32> = Vec::new();
        let (mut piece, mut at) = (0, 0);
        while at < text.len() {
            match self.added.iter().find(|(content, _)| text[at..].starts_with(content.as_str())) {
                Some((content, id)) => {
                    self.merge_into(&text[piece..at], &mut tokens);
                    tokens.push(*id);
                    at += content.len();
                    piece = at;
                }
                None => at += text[at..].chars().next().map_or(1, char::len_utf8),
            }
        }
        self.merge_into(&text[piece..], &mut tokens);
        let mut ids: Vec<i64> = tokens.into_iter().take(LENGTH - 1).map(i64::from).collect();
        ids.push(EOS);
        ids.resize(LENGTH, PAD);
        ids
    }

    fn merge_into(&self, text: &str, out: &mut Vec<u32>) {
        let mut tokens: Vec<u32> = Vec::new();
        for char in text.replace(' ', "▁").chars() {
            match self.chars.get(&char) {
                Some(id) => tokens.push(*id),
                None => tokens.extend(char.to_string().bytes().map(|byte| self.bytes[byte as usize])),
            }
        }

        while let Some((at, merged)) = tokens
            .windows(2)
            .enumerate()
            .filter_map(|(at, pair)| self.merges.get(&(pair[0], pair[1])).map(|(rank, merged)| (*rank, at, *merged)))
            .min()
            .map(|(_, at, merged)| (at, merged))
        {
            tokens.splice(at..at + 2, [merged]);
        }
        out.extend(tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> Tokenizer {
        let mut vocab: Vec<String> = ["<pad>", "<eos>", "<unk>", "b", "c", "▁", "ab", "abc", "▁a", "a"].map(String::from).to_vec();

        vocab.extend((0..=255u8).filter(|byte| *byte != b'{').map(|byte| format!("<0x{byte:02X}>")));
        let json = serde_json::json!({
            "model": {
                "vocab": vocab.iter().enumerate().map(|(id, token)| (token.clone(), id)).collect::<HashMap<_, _>>(),
                "merges": [["a", "b"], ["▁", "a"], ["ab", "c"]],
            },
            "added_tokens": [{ "id": 300, "content": "<b>" }, { "id": 301, "content": "<b>>" }],
        });
        Tokenizer::from_json(json.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn merges_by_rank_with_bytes_for_the_rest_and_eos_at_the_end() {
        let tokenizer = small();

        let ids = tokenizer.encode("abc AbZ{");
        assert_eq!(&ids[..7], &[7, 5, 6, 10 + 0x7A, 2, 1, 0]);
        assert_eq!(ids.len(), LENGTH);
        let long = tokenizer.encode(&"c".repeat(100));
        assert_eq!((long[LENGTH - 2], long[LENGTH - 1]), (4, 1), "cut to fit, <eos> kept");
    }

    #[test]
    fn an_added_token_is_one_id() {
        let tokenizer = small();
        assert_eq!(&tokenizer.encode("ab<b>c")[..4], &[6, 300, 4, 1]);
        assert_eq!(&tokenizer.encode("a<B>b")[..4], &[9, 300, 3, 1]);
        assert_eq!(&tokenizer.encode("<b>>")[..2], &[301, 1], "the longer one");
        assert_eq!(&tokenizer.encode("<b")[..4], &[10 + b'<' as i64, 3, 1, 0], "half of one is text");
    }

    #[test]
    #[ignore]
    fn gives_hugging_faces_ids() {
        let tokenizer = Tokenizer::load(Path::new(&std::env::var("SIGLIP_TOKENIZER").unwrap())).unwrap();
        for (text, want) in [
            ("bride with bouquet", &[57928, 675, 42783][..]),
            ("reiger", &[478, 5216]),
            ("a photo of a kitchen.", &[235250, 2686, 576, 476, 8926, 235265]),
            ("Sterretjes in de nacht", &[1568, 672, 8698, 575, 581, 90103]),
            ("café crème", &[81453, 60750]),
            ("ijsje 🍦", &[72873, 1792, 235248, 244567]),
            ("  two  spaces ", &[139, 9721, 139, 46633, 235248]),

            ("a<b>c", &[235250, 201, 235260]),
            ("<table>", &[169]),
            ("<strong>x</strong>", &[199, 235297, 208]),
            ("<EOS>", &[1]),
            ("<pad>", &[0]),
            ("sunset <b>beach</b> at dusk", &[78231, 235248, 201, 54901, 210, 696, 68171]),
            ("[@BOS@]", &[40341, 14084, 235348, 235307]),
            ("a\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\nb", &[235250, 138, 116, 235268]),
            ("<3 foto's", &[235322, 235304, 3775, 235303, 235256]),
        ] {
            let ids = tokenizer.encode(text);
            assert_eq!(&ids[..want.len() + 1], &[want, &[EOS]].concat()[..], "{text}");
            assert!(ids[want.len() + 1..].iter().all(|id| *id == PAD), "{text}");
        }
    }

    #[test]
    #[ignore]
    fn gives_hugging_faces_ids_for_a_corpus() {
        let tokenizer = Tokenizer::load(Path::new(&std::env::var("SIGLIP_TOKENIZER").unwrap())).unwrap();
        let corpus: Vec<(String, Vec<i64>)> = serde_json::from_slice(&std::fs::read(std::env::var("SIGLIP_CORPUS").unwrap()).unwrap()).unwrap();
        let different: Vec<String> = corpus
            .iter()
            .filter(|(text, want)| tokenizer.encode(text) != *want)
            .map(|(text, want)| format!("{text:?}: want {:?} got {:?}", &want[..8.min(want.len())], &tokenizer.encode(text)[..8]))
            .collect();
        assert!(different.is_empty(), "{} of {} differ:\n{}", different.len(), corpus.len(), different.join("\n"));
    }
}
