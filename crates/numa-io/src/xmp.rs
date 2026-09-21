use crate::export::{Description, ExportSettings};

pub fn description(settings: &ExportSettings, about: &Description, extra: Option<(String, String)>) -> Option<String> {
    let mut attributes = String::new();
    let mut elements = String::new();
    let list = |kind: &str, items: &[String]| {
        let items: String = items.iter().map(|item| format!("<rdf:li>{}</rdf:li>", escape(item))).collect();
        format!("<rdf:{kind}>{items}</rdf:{kind}>")
    };
    if !settings.creator.trim().is_empty() {
        elements += &format!("<dc:creator>{}</dc:creator>", list("Seq", &[settings.creator.trim().to_string()]));
    }
    if !settings.copyright.trim().is_empty() {
        let rights = escape(settings.copyright.trim());
        elements += &format!(r#"<dc:rights><rdf:Alt><rdf:li xml:lang="x-default">{rights}</rdf:li></rdf:Alt></dc:rights>"#);
    }
    if settings.keywords {
        if about.rating > 0 {
            attributes += &format!(r#" xmp:Rating="{}""#, about.rating.min(5));
        }
        let keywords: Vec<String> = about.people.iter().chain(&about.albums).cloned().collect();
        if !keywords.is_empty() {
            elements += &format!("<dc:subject>{}</dc:subject>", list("Bag", &keywords));
        }
        if !about.people.is_empty() {
            elements += &format!("<Iptc4xmpExt:PersonInImage>{}</Iptc4xmpExt:PersonInImage>", list("Bag", &about.people));
        }
    }
    if let Some((more_attributes, more_elements)) = extra {
        attributes += &more_attributes;
        elements += &more_elements;
    }
    if attributes.is_empty() && elements.is_empty() {
        return None;
    }
    Some(format!(
        r#"<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:Iptc4xmpExt="http://iptc.org/std/Iptc4xmpExt/2008-02-29/"{attributes}>{elements}</rdf:Description>"#
    ))
}

pub fn packet(description: &str) -> String {
    format!(
        r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Numa"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">{description}</rdf:RDF></x:xmpmeta><?xpacket end="w"?>"#
    )
}

pub fn segment(description: &str) -> Vec<u8> {
    let body = [b"http://ns.adobe.com/xap/1.0/\0".as_slice(), packet(description).as_bytes()].concat();
    let mut segment = vec![0xFF, 0xE1];
    segment.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    segment.extend_from_slice(&body);
    segment
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_and_the_photographer_are_described() {
        let settings = ExportSettings { creator: "Tijmen".into(), copyright: "© 2026 T & co".into(), ..ExportSettings::default() };
        let about = Description { rating: 4, people: vec!["Anna".into()], albums: vec!["Portfolio".into()] };
        let text = description(&settings, &about, None).unwrap();
        assert!(text.contains(r#"xmp:Rating="4""#));
        assert!(text.contains("<dc:creator><rdf:Seq><rdf:li>Tijmen</rdf:li></rdf:Seq></dc:creator>"));
        assert!(text.contains("© 2026 T &amp; co"));
        assert!(text.contains("<dc:subject><rdf:Bag><rdf:li>Anna</rdf:li><rdf:li>Portfolio</rdf:li></rdf:Bag></dc:subject>"));
        assert!(text.contains("<Iptc4xmpExt:PersonInImage><rdf:Bag><rdf:li>Anna</rdf:li>"));

        let quiet = ExportSettings { keywords: false, ..ExportSettings::default() };
        assert_eq!(description(&quiet, &about, None), None, "nothing asked for, nothing written");
    }
}
