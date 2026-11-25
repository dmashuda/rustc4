use super::*;

#[test]
fn named_custom_and_unknown() {
    let mut custom = BTreeMap::new();
    custom.insert("brand".to_string(), "#ffee00".to_string());
    assert_eq!(element("green", &custom).fill, "#428a4f");
    let brand = element("brand", &custom);
    assert_eq!(brand.fill, "#ffee00");
    assert_eq!(brand.stroke, "#bfb300");
    // Dark text on a light color.
    assert!(brand.text.as_str() < "#444444", "{}", brand.text);
    assert_eq!(element("nope", &custom), element("primary", &custom));
    assert_eq!(line("gray", &custom), LINE_GRAY);
    assert_eq!(line("red", &custom), "#ac4d39");
}
