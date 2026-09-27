use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ObstacleKind {
    Physical,
    Cosmetic,
}

impl Default for ObstacleKind {
    fn default() -> Self {
        Self::Physical
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ObstacleShape {
    Polygon {
        vertices: Vec<[f32; 2]>,
    },
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        rotation: f32,
    },
    Circle {
        center: [f32; 2],
        radius: f32,
    },
}

impl Default for ObstacleShape {
    fn default() -> Self {
        Self::Polygon {
            vertices: vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Obstacle {
    pub id: String,
    #[serde(default)]
    pub kind: ObstacleKind,
    #[serde(default)]
    pub shape: ObstacleShape,
    #[serde(flatten)]
    pub extra: IndexMap<String, serde_yaml::Value>,
}

impl Obstacle {
    pub fn new_polygon(id: String, kind: ObstacleKind, vertices: Vec<[f32; 2]>) -> Self {
        Self {
            id,
            kind,
            shape: ObstacleShape::Polygon { vertices },
            extra: IndexMap::new(),
        }
    }

    pub fn new_rectangle(id: String, kind: ObstacleKind, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            id,
            kind,
            shape: ObstacleShape::Rectangle {
                x,
                y,
                width,
                height,
                rotation: 0.0,
            },
            extra: IndexMap::new(),
        }
    }

    pub fn new_circle(id: String, kind: ObstacleKind, center: [f32; 2], radius: f32) -> Self {
        Self {
            id,
            kind,
            shape: ObstacleShape::Circle { center, radius },
            extra: IndexMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MissionPoint {
    pub x: f32,
    pub y: f32,
    #[serde(flatten)]
    pub extra: IndexMap<String, serde_yaml::Value>,
}

impl MissionPoint {
    pub fn new(x: f32, y: f32) -> Self {
        let mut extra = IndexMap::new();
        extra.insert("margin".to_string(), serde_yaml::Value::from(0.5));
        extra.insert("odom_speed".to_string(), serde_yaml::Value::from(0.5));
        extra.insert("vision_speed".to_string(), serde_yaml::Value::from(0.3));
        extra.insert("dodge".to_string(), serde_yaml::Value::from(true));
        extra.insert("has_cone".to_string(), serde_yaml::Value::from(false));
        extra.insert("use_bumper".to_string(), serde_yaml::Value::from(true));
        Self { x, y, extra }
    }

    pub fn get_margin(&self) -> f32 {
        self.extra
            .get("margin")
            .and_then(|v| v.as_f64().map(|f| f as f32))
            .unwrap_or(0.5)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MissionData {
    pub points: Vec<MissionPoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obstacles: Vec<Obstacle>,
}

impl MissionData {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let data: MissionData = serde_yaml::from_str(&content)?;
        Ok(data)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let yaml_str = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml_str)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_existing_mission_points() {
        let path = "mission_points.yaml";
        let data = MissionData::load_from_file(path).expect("Should load mission_points.yaml");
        assert!(!data.points.is_empty(), "Points array should not be empty");
        assert_eq!(data.points[0].x, 3.0);
        assert_eq!(data.points[0].y, 0.0);
        assert!(data.points[0].extra.contains_key("odom_speed"));
    }

    #[test]
    fn test_dynamic_parameters_roundtrip() {
        let mut data = MissionData::default();
        let mut pt = MissionPoint::new(10.0, 20.0);
        pt.extra.insert("custom_field".to_string(), serde_yaml::Value::String("test_val".to_string()));
        data.points.push(pt);

        let yaml = serde_yaml::to_string(&data).unwrap();
        assert!(yaml.contains("custom_field: test_val"));

        let loaded: MissionData = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(loaded.points[0].extra.get("custom_field").unwrap().as_str().unwrap(), "test_val");
    }
}
