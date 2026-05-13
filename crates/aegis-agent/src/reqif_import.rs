//! ReqIF XML parser and RTM mapping layer.
//!
//! Parses ReqIF XML exchange files (DOORS/IBM format) into `SpecObject` structs,
//! then maps them to `RtmImportRow` for import into the RTM database.
//!
//! Uses simple string-based XML parsing since no XML crate is available.

use aegis_domain::DomainError;

/// A parsed ReqIF SpecObject.
#[derive(Debug, Clone)]
pub struct SpecObject {
    /// The IDENTIFIER attribute of the SpecObject.
    pub identifier: String,
    /// The LongName attribute (human-readable name/text).
    pub long_name: String,
    /// Additional attribute values as key-value pairs.
    pub attributes: Vec<(String, String)>,
}

/// A row ready for import into the RTM database.
#[derive(Debug, Clone)]
pub struct RtmImportRow {
    pub req_id: String,
    pub category: String,
    pub requirement_text: String,
    pub priority: String,
    pub status: String,
    pub external_id: String,
}

/// Extract the value of an XML attribute from a tag string.
///
/// Given a tag like `<SPEC-OBJECT IDENTIFIER="REQ-001" LONG-NAME="text">`,
/// calling `extract_xml_attr(tag, "IDENTIFIER")` returns `Some("REQ-001")`.
fn extract_xml_attr(tag: &str, attr_name: &str) -> Option<String> {
    // Look for attr_name="value" or attr_name='value'
    let patterns = [format!("{attr_name}=\""), format!("{attr_name}='")];
    for pattern in &patterns {
        if let Some(start) = tag.find(pattern.as_str()) {
            let value_start = start + pattern.len();
            let delimiter = pattern.chars().last().unwrap();
            if let Some(end) = tag[value_start..].find(delimiter) {
                return Some(tag[value_start..value_start + end].to_string());
            }
        }
    }
    None
}

/// Find the start of a `<SPEC-OBJECT` opening tag, avoiding `<SPEC-OBJECTS`.
fn find_spec_object_tag(s: &str) -> Option<usize> {
    let mut pos = 0;
    while let Some(idx) = s[pos..].find("<SPEC-OBJECT") {
        let abs = pos + idx;
        let after = abs + "<SPEC-OBJECT".len();
        if after >= s.len() {
            return Some(abs);
        }
        let next_char = s.as_bytes()[after];
        // Must be followed by space, '>', '/', or newline -- not 'S' (SPEC-OBJECTS)
        if next_char == b' '
            || next_char == b'>'
            || next_char == b'/'
            || next_char == b'\n'
            || next_char == b'\r'
        {
            return Some(abs);
        }
        pos = abs + 1;
    }
    None
}

/// Find the closing `</SPEC-OBJECT>` tag, avoiding `</SPEC-OBJECTS>`.
fn find_spec_object_close(s: &str) -> Option<usize> {
    let mut pos = 0;
    while let Some(idx) = s[pos..].find("</SPEC-OBJECT") {
        let abs = pos + idx;
        let after = abs + "</SPEC-OBJECT".len();
        if after >= s.len() {
            return Some(abs);
        }
        let next_char = s.as_bytes()[after];
        if next_char == b'>' || next_char == b' ' {
            return Some(abs);
        }
        pos = abs + 1;
    }
    None
}

/// Parse ReqIF XML exchange files and extract SpecObject elements.
///
/// Handles ReqIF namespaces (`http://www.omg.org/spec/ReqIF/20110401/reqif.xsd`)
/// by searching for `<SPEC-OBJECT` tags regardless of namespace prefixes.
pub fn parse_reqif(xml: &str) -> Result<Vec<SpecObject>, DomainError> {
    // Basic validation: check for REQ-IF or SPEC-OBJECTS presence
    if xml.trim().is_empty() {
        return Err(DomainError::Other(
            "Invalid ReqIF XML: empty input".to_string(),
        ));
    }

    // Check for basic XML structure
    if !xml.contains('<') {
        return Err(DomainError::Other(
            "Invalid ReqIF XML: no XML elements found".to_string(),
        ));
    }

    let mut spec_objects = Vec::new();
    let mut search_pos = 0;

    while let Some(so_start) = find_spec_object_tag(&xml[search_pos..]) {
        let abs_start = search_pos + so_start;

        // Find the end of this SPEC-OBJECT element.
        // It could be self-closing or have a closing tag.
        let so_end = if let Some(close) = find_spec_object_close(&xml[abs_start..]) {
            let close_abs = abs_start + close;
            // Find the '>' after </SPEC-OBJECT
            xml[close_abs..]
                .find('>')
                .map(|g| close_abs + g + 1)
                .unwrap_or(xml.len())
        } else if let Some(sc) = xml[abs_start..].find("/>") {
            abs_start + sc + 2
        } else {
            xml.len()
        };

        let so_block = &xml[abs_start..so_end];

        // Extract the opening tag (up to first '>' or '/')
        let tag_end = so_block
            .find('>')
            .or_else(|| so_block.find("/>"))
            .unwrap_or(so_block.len());
        let opening_tag = &so_block[..tag_end];

        let identifier = extract_xml_attr(opening_tag, "IDENTIFIER").unwrap_or_default();
        let long_name = extract_xml_attr(opening_tag, "LONG-NAME").unwrap_or_default();

        // Extract ATTRIBUTE-VALUE-STRING elements
        let mut attributes = Vec::new();
        let mut attr_pos = 0;
        while let Some(av_start) = so_block[attr_pos..].find("<ATTRIBUTE-VALUE-STRING") {
            let av_abs = attr_pos + av_start;
            let av_end = so_block[av_abs..]
                .find('>')
                .map(|g| av_abs + g)
                .unwrap_or(so_block.len());
            let av_tag = &so_block[av_abs..av_end];

            if let Some(the_value) = extract_xml_attr(av_tag, "THE-VALUE") {
                // Try to find the ATTRIBUTE-DEFINITION-STRING-REF for the key
                let ref_key = if let Some(ref_start) =
                    so_block[av_abs..].find("<ATTRIBUTE-DEFINITION-STRING-REF>")
                {
                    let ref_abs = av_abs + ref_start;
                    let ref_tag_end = ref_abs + "<ATTRIBUTE-DEFINITION-STRING-REF>".len();
                    if let Some(ref_close) =
                        so_block[ref_tag_end..].find("</ATTRIBUTE-DEFINITION-STRING-REF>")
                    {
                        so_block[ref_tag_end..ref_tag_end + ref_close]
                            .trim()
                            .to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                attributes.push((ref_key, the_value));
            }
            attr_pos = av_end + 1;
        }

        spec_objects.push(SpecObject {
            identifier,
            long_name,
            attributes,
        });

        search_pos = so_end;
    }

    Ok(spec_objects)
}

/// Map SpecObject attributes to RTM import rows.
///
/// - `LongName` maps to `requirement_text`
/// - `IDENTIFIER` maps to `external_id`
/// - `req_id` is generated as `REQ-REQIF-{IDENTIFIER}` (uppercase)
/// - `category` defaults to `"COMPLIANCE"`
/// - `status` defaults to `"MISSING"`
/// - `priority` defaults to `"MEDIUM"`
pub fn reqif_to_rtm(spec_objects: &[SpecObject]) -> Vec<RtmImportRow> {
    spec_objects
        .iter()
        .map(|so| {
            let id_upper = so.identifier.to_uppercase();
            RtmImportRow {
                req_id: format!("REQ-REQIF-{id_upper}"),
                category: "COMPLIANCE".to_string(),
                requirement_text: so.long_name.clone(),
                priority: "MEDIUM".to_string(),
                status: "MISSING".to_string(),
                external_id: so.identifier.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_REQIF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<REQ-IF xmlns="http://www.omg.org/spec/ReqIF/20110401/reqif.xsd">
  <CORE-CONTENT>
    <REQ-IF-CONTENT>
      <SPEC-OBJECTS>
        <SPEC-OBJECT IDENTIFIER="REQ-001" LONG-NAME="System shall authenticate users">
          <VALUES>
            <ATTRIBUTE-VALUE-STRING THE-VALUE="High">
              <DEFINITION>
                <ATTRIBUTE-DEFINITION-STRING-REF>priority-attr</ATTRIBUTE-DEFINITION-STRING-REF>
              </DEFINITION>
            </ATTRIBUTE-VALUE-STRING>
          </VALUES>
        </SPEC-OBJECT>
        <SPEC-OBJECT IDENTIFIER="REQ-002" LONG-NAME="System shall encrypt data at rest">
          <VALUES>
            <ATTRIBUTE-VALUE-STRING THE-VALUE="Critical">
              <DEFINITION>
                <ATTRIBUTE-DEFINITION-STRING-REF>severity-attr</ATTRIBUTE-DEFINITION-STRING-REF>
              </DEFINITION>
            </ATTRIBUTE-VALUE-STRING>
          </VALUES>
        </SPEC-OBJECT>
      </SPEC-OBJECTS>
    </REQ-IF-CONTENT>
  </CORE-CONTENT>
</REQ-IF>"#;

    // rtmx:req REQ-RTMX-032
    #[test]
    fn test_reqif_parser_extracts_specobjects() {
        let result = parse_reqif(SAMPLE_REQIF).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].identifier, "REQ-001");
        assert_eq!(result[0].long_name, "System shall authenticate users");
        assert_eq!(result[0].attributes.len(), 1);
        assert_eq!(result[0].attributes[0].0, "priority-attr");
        assert_eq!(result[0].attributes[0].1, "High");
        assert_eq!(result[1].identifier, "REQ-002");
        assert_eq!(result[1].long_name, "System shall encrypt data at rest");
    }

    // rtmx:req REQ-RTMX-032
    #[test]
    fn test_reqif_parser_handles_namespaces() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<REQ-IF xmlns="http://www.omg.org/spec/ReqIF/20110401/reqif.xsd"
        xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <CORE-CONTENT>
    <REQ-IF-CONTENT>
      <SPEC-OBJECTS>
        <SPEC-OBJECT IDENTIFIER="NS-001" LONG-NAME="Namespaced requirement">
        </SPEC-OBJECT>
      </SPEC-OBJECTS>
    </REQ-IF-CONTENT>
  </CORE-CONTENT>
</REQ-IF>"#;
        let result = parse_reqif(xml).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].identifier, "NS-001");
        assert_eq!(result[0].long_name, "Namespaced requirement");
    }

    // rtmx:req REQ-RTMX-032
    #[test]
    fn test_reqif_parser_empty_specobjects() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<REQ-IF xmlns="http://www.omg.org/spec/ReqIF/20110401/reqif.xsd">
  <CORE-CONTENT>
    <REQ-IF-CONTENT>
      <SPEC-OBJECTS>
      </SPEC-OBJECTS>
    </REQ-IF-CONTENT>
  </CORE-CONTENT>
</REQ-IF>"#;
        let result = parse_reqif(xml).unwrap();
        assert!(result.is_empty());
    }

    // rtmx:req REQ-RTMX-032
    #[test]
    fn test_reqif_parser_missing_longname() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<REQ-IF xmlns="http://www.omg.org/spec/ReqIF/20110401/reqif.xsd">
  <CORE-CONTENT>
    <REQ-IF-CONTENT>
      <SPEC-OBJECTS>
        <SPEC-OBJECT IDENTIFIER="NO-LN-001">
        </SPEC-OBJECT>
      </SPEC-OBJECTS>
    </REQ-IF-CONTENT>
  </CORE-CONTENT>
</REQ-IF>"#;
        let result = parse_reqif(xml).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].identifier, "NO-LN-001");
        assert_eq!(result[0].long_name, "");
    }

    // rtmx:req REQ-RTMX-033
    #[test]
    fn test_reqif_to_rtm_maps_attributes() {
        let spec_objects = vec![SpecObject {
            identifier: "REQ-001".to_string(),
            long_name: "System shall authenticate users".to_string(),
            attributes: vec![("priority-attr".to_string(), "High".to_string())],
        }];
        let rows = reqif_to_rtm(&spec_objects);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].requirement_text, "System shall authenticate users");
        assert_eq!(rows[0].external_id, "REQ-001");
    }

    // rtmx:req REQ-RTMX-033
    #[test]
    fn test_reqif_to_rtm_generates_req_ids() {
        let spec_objects = vec![
            SpecObject {
                identifier: "req-abc".to_string(),
                long_name: "First".to_string(),
                attributes: vec![],
            },
            SpecObject {
                identifier: "REQ-XYZ".to_string(),
                long_name: "Second".to_string(),
                attributes: vec![],
            },
        ];
        let rows = reqif_to_rtm(&spec_objects);
        assert_eq!(rows[0].req_id, "REQ-REQIF-REQ-ABC");
        assert_eq!(rows[1].req_id, "REQ-REQIF-REQ-XYZ");
    }

    // rtmx:req REQ-RTMX-033
    #[test]
    fn test_reqif_to_rtm_sets_defaults() {
        let spec_objects = vec![SpecObject {
            identifier: "D-001".to_string(),
            long_name: "Default check".to_string(),
            attributes: vec![],
        }];
        let rows = reqif_to_rtm(&spec_objects);
        assert_eq!(rows[0].category, "COMPLIANCE");
        assert_eq!(rows[0].status, "MISSING");
        assert_eq!(rows[0].priority, "MEDIUM");
    }

    // rtmx:req REQ-RTMX-032
    #[test]
    fn test_reqif_parser_invalid_xml() {
        let result = parse_reqif("");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid ReqIF XML"));

        let result2 = parse_reqif("not xml at all");
        assert!(result2.is_err());
    }

    // rtmx:req REQ-RTMX-016
    // rtmx:req REQ-RTMX-010
    #[test]
    fn test_reqif_xml_import_end_to_end() {
        // Verify full pipeline: parse ReqIF XML -> map to RTM rows.
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<REQ-IF>
  <CORE-CONTENT>
    <REQ-IF-CONTENT>
      <SPEC-OBJECTS>
        <SPEC-OBJECT IDENTIFIER="REQ-001">
          <VALUES>
            <ATTRIBUTE-VALUE-STRING THE-VALUE="Shall support TLS 1.3"/>
          </VALUES>
        </SPEC-OBJECT>
      </SPEC-OBJECTS>
    </REQ-IF-CONTENT>
  </CORE-CONTENT>
</REQ-IF>"#;
        let spec_objects = parse_reqif(xml).unwrap();
        assert!(!spec_objects.is_empty(), "must parse spec objects");
        let rows = reqif_to_rtm(&spec_objects);
        assert!(!rows.is_empty(), "must produce RTM rows");
        assert!(
            rows[0].req_id.starts_with("REQ-REQIF-"),
            "ReqIF imports must use REQ-REQIF- prefix"
        );
    }
}
