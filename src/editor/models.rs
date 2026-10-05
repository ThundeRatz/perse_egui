use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ObstacleKind {
    #[default]
    Physical,
    Cosmetic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ObstacleShape {
    Polygon {
        vertices: Vec<[f32; 2]>,
    },
    Line {
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

    pub fn new_line(id: String, kind: ObstacleKind, vertices: Vec<[f32; 2]>) -> Self {
        Self {
            id,
            kind,
            shape: ObstacleShape::Line { vertices },
            extra: IndexMap::new(),
        }
    }

    pub fn new_rectangle(
        id: String,
        kind: ObstacleKind,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Self {
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
        #[cfg(not(target_arch = "wasm32"))]
        {
            let content = std::fs::read_to_string(path)?;
            let data: MissionData = serde_yaml::from_str(&content)?;
            Ok(data)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            Ok(Self::default())
        }
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path_ref = path.as_ref();
            if let Some(parent) = path_ref.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            let yaml_str = serde_yaml::to_string(self)?;
            std::fs::write(path_ref, yaml_str)?;
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MissionSet {
    pub id: String,
    pub name: String,
    pub data: MissionData,
}

impl MissionSet {
    pub fn new(id: impl Into<String>, name: impl Into<String>, data: MissionData) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            data,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MissionSetCollection {
    pub active_set_id: String,
    pub sets: Vec<MissionSet>,
}

impl Default for MissionSetCollection {
    fn default() -> Self {
        let initial_data = MissionData::load_from_file("mission_points.yaml").unwrap_or_default();
        let default_set = MissionSet::new("default", "Missão Padrão", initial_data);
        Self {
            active_set_id: "default".to_string(),
            sets: vec![default_set],
        }
    }
}

impl MissionSetCollection {
    pub fn active_data(&self) -> Option<&MissionData> {
        self.sets
            .iter()
            .find(|s| s.id == self.active_set_id)
            .map(|s| &s.data)
    }

    pub fn active_data_mut(&mut self) -> Option<&mut MissionData> {
        self.sets
            .iter_mut()
            .find(|s| s.id == self.active_set_id)
            .map(|s| &mut s.data)
    }

    pub fn set_active(&mut self, id: &str) -> bool {
        if self.sets.iter().any(|s| s.id == id) {
            self.active_set_id = id.to_string();
            true
        } else {
            false
        }
    }

    pub fn add_set(&mut self, name: impl Into<String>, data: MissionData) -> String {
        static SET_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let name_str = name.into();
        let counter = SET_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = format!("set_{}_{}", self.sets.len() + 1, counter);
        let set = MissionSet::new(id.clone(), name_str, data);
        self.sets.push(set);
        self.active_set_id = id.clone();
        id
    }

    pub fn duplicate_active(&mut self) -> Option<String> {
        let active_data = self.active_data()?.clone();
        let active_name = self
            .sets
            .iter()
            .find(|s| s.id == self.active_set_id)?
            .name
            .clone();
        let new_name = format!("{} (Cópia)", active_name);
        Some(self.add_set(new_name, active_data))
    }

    pub fn delete_active(&mut self) -> bool {
        if self.sets.len() <= 1 {
            return false;
        }
        if let Some(pos) = self.sets.iter().position(|s| s.id == self.active_set_id) {
            self.sets.remove(pos);
            let next_id = self.sets[0].id.clone();
            self.active_set_id = next_id;
            true
        } else {
            false
        }
    }

    pub fn rename_active(&mut self, new_name: impl Into<String>) {
        if let Some(set) = self.sets.iter_mut().find(|s| s.id == self.active_set_id) {
            set.name = new_name.into();
        }
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let content = std::fs::read_to_string(path)?;
            let col: MissionSetCollection = serde_json::from_str(&content)?;
            Ok(col)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            Ok(Self::default())
        }
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path_ref = path.as_ref();
            if let Some(parent) = path_ref.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            let json_str = serde_json::to_string_pretty(self)?;
            std::fs::write(path_ref, json_str)?;
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_existing_mission_points() {
        let sample_yaml = r#"
points:
  - x: 10.0
    y: 20.0
    odom_speed: 1.5
obstacles: []
"#;
        let temp_path =
            std::env::temp_dir().join(format!("test_mission_points_{}.yaml", std::process::id()));
        std::fs::write(&temp_path, sample_yaml).expect("Failed to write temp test yaml");

        let data =
            MissionData::load_from_file(&temp_path).expect("Should load mission_points.yaml");
        assert!(!data.points.is_empty(), "Points array should not be empty");
        assert!(data.points[0].extra.contains_key("odom_speed"));

        let _ = std::fs::remove_file(temp_path);
    }

    #[test]
    fn test_dynamic_parameters_roundtrip() {
        let mut data = MissionData::default();
        let mut pt = MissionPoint::new(10.0, 20.0);
        pt.extra.insert(
            "custom_field".to_string(),
            serde_yaml::Value::String("test_val".to_string()),
        );
        data.points.push(pt);

        let yaml = serde_yaml::to_string(&data).unwrap();
        assert!(yaml.contains("custom_field: test_val"));

        let loaded: MissionData = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            loaded.points[0]
                .extra
                .get("custom_field")
                .unwrap()
                .as_str()
                .unwrap(),
            "test_val"
        );
    }

    #[test]
    fn test_line_obstacle_roundtrip() {
        let mut data = MissionData::default();
        let obs = Obstacle::new_line(
            "obs_1".to_string(),
            ObstacleKind::Physical,
            vec![[0.0, 0.0], [5.0, 5.0]],
        );
        data.obstacles.push(obs);

        let yaml = serde_yaml::to_string(&data).unwrap();
        assert!(yaml.contains("line"));

        let loaded: MissionData = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(loaded.obstacles.len(), 1);
        if let ObstacleShape::Line { vertices } = &loaded.obstacles[0].shape {
            assert_eq!(vertices.len(), 2);
            assert_eq!(vertices[1], [5.0, 5.0]);
        } else {
            panic!("Expected ObstacleShape::Line");
        }
    }

    #[test]
    fn test_mission_set_collection_persistence_roundtrip() {
        let mut collection = MissionSetCollection::default();
        let mut data1 = MissionData::default();
        data1.points.push(MissionPoint::new(1.0, 2.0));
        data1.points.push(MissionPoint::new(3.0, 4.0));

        let mut data2 = MissionData::default();
        data2.points.push(MissionPoint::new(10.0, 20.0));
        data2.obstacles.push(Obstacle::new_circle(
            "obs1".to_string(),
            ObstacleKind::Physical,
            [5.0, 5.0],
            2.5,
        ));

        let id1 = collection.add_set("Rota A", data1);
        let id2 = collection.add_set("Rota B", data2);
        collection.set_active(&id2);

        let temp_path = std::env::temp_dir().join("test_mission_collection.json");
        collection
            .save_to_file(&temp_path)
            .expect("Should save collection");

        let loaded =
            MissionSetCollection::load_from_file(&temp_path).expect("Should load collection");
        assert_eq!(loaded.active_set_id, id2);
        assert_eq!(loaded.sets.len(), 3); // default + Rota A + Rota B

        let active = loaded.active_data().expect("Should have active data");
        assert_eq!(active.points.len(), 1);
        assert_eq!(active.points[0].x, 10.0);
        assert_eq!(active.obstacles.len(), 1);

        let set_a = loaded
            .sets
            .iter()
            .find(|s| s.id == id1)
            .expect("Should find Rota A");
        assert_eq!(set_a.name, "Rota A");
        assert_eq!(set_a.data.points.len(), 2);

        let _ = std::fs::remove_file(temp_path);
    }
}
