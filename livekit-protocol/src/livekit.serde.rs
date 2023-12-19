// @generated
impl serde::Serialize for ActiveSpeakerUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.speakers.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ActiveSpeakerUpdate", len)?;
        if !self.speakers.is_empty() {
            struct_ser.serialize_field("speakers", &self.speakers)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ActiveSpeakerUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "speakers",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Speakers,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "speakers" => Ok(GeneratedField::Speakers),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ActiveSpeakerUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ActiveSpeakerUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ActiveSpeakerUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut speakers__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Speakers => {
                            if speakers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("speakers"));
                            }
                            speakers__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ActiveSpeakerUpdate {
                    speakers: speakers__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ActiveSpeakerUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AddTrackRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.cid.is_empty() {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        if self.r#type != 0 {
            len += 1;
        }
        if self.width != 0 {
            len += 1;
        }
        if self.height != 0 {
            len += 1;
        }
        if self.muted {
            len += 1;
        }
        if self.disable_dtx {
            len += 1;
        }
        if self.source != 0 {
            len += 1;
        }
        if !self.layers.is_empty() {
            len += 1;
        }
        if !self.simulcast_codecs.is_empty() {
            len += 1;
        }
        if !self.sid.is_empty() {
            len += 1;
        }
        if self.stereo {
            len += 1;
        }
        if self.disable_red {
            len += 1;
        }
        if self.encryption != 0 {
            len += 1;
        }
        if !self.stream.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.AddTrackRequest", len)?;
        if !self.cid.is_empty() {
            struct_ser.serialize_field("cid", &self.cid)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if self.r#type != 0 {
            let v = TrackType::from_i32(self.r#type)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.r#type)))?;
            struct_ser.serialize_field("type", &v)?;
        }
        if self.width != 0 {
            struct_ser.serialize_field("width", &self.width)?;
        }
        if self.height != 0 {
            struct_ser.serialize_field("height", &self.height)?;
        }
        if self.muted {
            struct_ser.serialize_field("muted", &self.muted)?;
        }
        if self.disable_dtx {
            struct_ser.serialize_field("disableDtx", &self.disable_dtx)?;
        }
        if self.source != 0 {
            let v = TrackSource::from_i32(self.source)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.source)))?;
            struct_ser.serialize_field("source", &v)?;
        }
        if !self.layers.is_empty() {
            struct_ser.serialize_field("layers", &self.layers)?;
        }
        if !self.simulcast_codecs.is_empty() {
            struct_ser.serialize_field("simulcastCodecs", &self.simulcast_codecs)?;
        }
        if !self.sid.is_empty() {
            struct_ser.serialize_field("sid", &self.sid)?;
        }
        if self.stereo {
            struct_ser.serialize_field("stereo", &self.stereo)?;
        }
        if self.disable_red {
            struct_ser.serialize_field("disableRed", &self.disable_red)?;
        }
        if self.encryption != 0 {
            let v = encryption::Type::from_i32(self.encryption)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.encryption)))?;
            struct_ser.serialize_field("encryption", &v)?;
        }
        if !self.stream.is_empty() {
            struct_ser.serialize_field("stream", &self.stream)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AddTrackRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "cid",
            "name",
            "type",
            "width",
            "height",
            "muted",
            "disable_dtx",
            "disableDtx",
            "source",
            "layers",
            "simulcast_codecs",
            "simulcastCodecs",
            "sid",
            "stereo",
            "disable_red",
            "disableRed",
            "encryption",
            "stream",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Cid,
            Name,
            Type,
            Width,
            Height,
            Muted,
            DisableDtx,
            Source,
            Layers,
            SimulcastCodecs,
            Sid,
            Stereo,
            DisableRed,
            Encryption,
            Stream,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "cid" => Ok(GeneratedField::Cid),
                            "name" => Ok(GeneratedField::Name),
                            "type" => Ok(GeneratedField::Type),
                            "width" => Ok(GeneratedField::Width),
                            "height" => Ok(GeneratedField::Height),
                            "muted" => Ok(GeneratedField::Muted),
                            "disableDtx" | "disable_dtx" => Ok(GeneratedField::DisableDtx),
                            "source" => Ok(GeneratedField::Source),
                            "layers" => Ok(GeneratedField::Layers),
                            "simulcastCodecs" | "simulcast_codecs" => Ok(GeneratedField::SimulcastCodecs),
                            "sid" => Ok(GeneratedField::Sid),
                            "stereo" => Ok(GeneratedField::Stereo),
                            "disableRed" | "disable_red" => Ok(GeneratedField::DisableRed),
                            "encryption" => Ok(GeneratedField::Encryption),
                            "stream" => Ok(GeneratedField::Stream),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AddTrackRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.AddTrackRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<AddTrackRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut cid__ = None;
                let mut name__ = None;
                let mut r#type__ = None;
                let mut width__ = None;
                let mut height__ = None;
                let mut muted__ = None;
                let mut disable_dtx__ = None;
                let mut source__ = None;
                let mut layers__ = None;
                let mut simulcast_codecs__ = None;
                let mut sid__ = None;
                let mut stereo__ = None;
                let mut disable_red__ = None;
                let mut encryption__ = None;
                let mut stream__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Cid => {
                            if cid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("cid"));
                            }
                            cid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map.next_value::<TrackType>()? as i32);
                        }
                        GeneratedField::Width => {
                            if width__.is_some() {
                                return Err(serde::de::Error::duplicate_field("width"));
                            }
                            width__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Muted => {
                            if muted__.is_some() {
                                return Err(serde::de::Error::duplicate_field("muted"));
                            }
                            muted__ = Some(map.next_value()?);
                        }
                        GeneratedField::DisableDtx => {
                            if disable_dtx__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableDtx"));
                            }
                            disable_dtx__ = Some(map.next_value()?);
                        }
                        GeneratedField::Source => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source__ = Some(map.next_value::<TrackSource>()? as i32);
                        }
                        GeneratedField::Layers => {
                            if layers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("layers"));
                            }
                            layers__ = Some(map.next_value()?);
                        }
                        GeneratedField::SimulcastCodecs => {
                            if simulcast_codecs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("simulcastCodecs"));
                            }
                            simulcast_codecs__ = Some(map.next_value()?);
                        }
                        GeneratedField::Sid => {
                            if sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sid"));
                            }
                            sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Stereo => {
                            if stereo__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stereo"));
                            }
                            stereo__ = Some(map.next_value()?);
                        }
                        GeneratedField::DisableRed => {
                            if disable_red__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableRed"));
                            }
                            disable_red__ = Some(map.next_value()?);
                        }
                        GeneratedField::Encryption => {
                            if encryption__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encryption"));
                            }
                            encryption__ = Some(map.next_value::<encryption::Type>()? as i32);
                        }
                        GeneratedField::Stream => {
                            if stream__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stream"));
                            }
                            stream__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(AddTrackRequest {
                    cid: cid__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                    r#type: r#type__.unwrap_or_default(),
                    width: width__.unwrap_or_default(),
                    height: height__.unwrap_or_default(),
                    muted: muted__.unwrap_or_default(),
                    disable_dtx: disable_dtx__.unwrap_or_default(),
                    source: source__.unwrap_or_default(),
                    layers: layers__.unwrap_or_default(),
                    simulcast_codecs: simulcast_codecs__.unwrap_or_default(),
                    sid: sid__.unwrap_or_default(),
                    stereo: stereo__.unwrap_or_default(),
                    disable_red: disable_red__.unwrap_or_default(),
                    encryption: encryption__.unwrap_or_default(),
                    stream: stream__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.AddTrackRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AliOssUpload {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.access_key.is_empty() {
            len += 1;
        }
        if !self.secret.is_empty() {
            len += 1;
        }
        if !self.region.is_empty() {
            len += 1;
        }
        if !self.endpoint.is_empty() {
            len += 1;
        }
        if !self.bucket.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.AliOSSUpload", len)?;
        if !self.access_key.is_empty() {
            struct_ser.serialize_field("accessKey", &self.access_key)?;
        }
        if !self.secret.is_empty() {
            struct_ser.serialize_field("secret", &self.secret)?;
        }
        if !self.region.is_empty() {
            struct_ser.serialize_field("region", &self.region)?;
        }
        if !self.endpoint.is_empty() {
            struct_ser.serialize_field("endpoint", &self.endpoint)?;
        }
        if !self.bucket.is_empty() {
            struct_ser.serialize_field("bucket", &self.bucket)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AliOssUpload {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "access_key",
            "accessKey",
            "secret",
            "region",
            "endpoint",
            "bucket",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AccessKey,
            Secret,
            Region,
            Endpoint,
            Bucket,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "accessKey" | "access_key" => Ok(GeneratedField::AccessKey),
                            "secret" => Ok(GeneratedField::Secret),
                            "region" => Ok(GeneratedField::Region),
                            "endpoint" => Ok(GeneratedField::Endpoint),
                            "bucket" => Ok(GeneratedField::Bucket),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AliOssUpload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.AliOSSUpload")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<AliOssUpload, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut access_key__ = None;
                let mut secret__ = None;
                let mut region__ = None;
                let mut endpoint__ = None;
                let mut bucket__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::AccessKey => {
                            if access_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accessKey"));
                            }
                            access_key__ = Some(map.next_value()?);
                        }
                        GeneratedField::Secret => {
                            if secret__.is_some() {
                                return Err(serde::de::Error::duplicate_field("secret"));
                            }
                            secret__ = Some(map.next_value()?);
                        }
                        GeneratedField::Region => {
                            if region__.is_some() {
                                return Err(serde::de::Error::duplicate_field("region"));
                            }
                            region__ = Some(map.next_value()?);
                        }
                        GeneratedField::Endpoint => {
                            if endpoint__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endpoint"));
                            }
                            endpoint__ = Some(map.next_value()?);
                        }
                        GeneratedField::Bucket => {
                            if bucket__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bucket"));
                            }
                            bucket__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(AliOssUpload {
                    access_key: access_key__.unwrap_or_default(),
                    secret: secret__.unwrap_or_default(),
                    region: region__.unwrap_or_default(),
                    endpoint: endpoint__.unwrap_or_default(),
                    bucket: bucket__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.AliOSSUpload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AudioCodec {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::DefaultAc => "DEFAULT_AC",
            Self::Opus => "OPUS",
            Self::Aac => "AAC",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for AudioCodec {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "DEFAULT_AC",
            "OPUS",
            "AAC",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AudioCodec;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(AudioCodec::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(AudioCodec::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "DEFAULT_AC" => Ok(AudioCodec::DefaultAc),
                    "OPUS" => Ok(AudioCodec::Opus),
                    "AAC" => Ok(AudioCodec::Aac),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for AutoParticipantEgress {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.file_outputs.is_empty() {
            len += 1;
        }
        if !self.segment_outputs.is_empty() {
            len += 1;
        }
        if self.options.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.AutoParticipantEgress", len)?;
        if !self.file_outputs.is_empty() {
            struct_ser.serialize_field("fileOutputs", &self.file_outputs)?;
        }
        if !self.segment_outputs.is_empty() {
            struct_ser.serialize_field("segmentOutputs", &self.segment_outputs)?;
        }
        if let Some(v) = self.options.as_ref() {
            match v {
                auto_participant_egress::Options::Preset(v) => {
                    let v = EncodingOptionsPreset::from_i32(*v)
                        .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("preset", &v)?;
                }
                auto_participant_egress::Options::Advanced(v) => {
                    struct_ser.serialize_field("advanced", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AutoParticipantEgress {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "file_outputs",
            "fileOutputs",
            "segment_outputs",
            "segmentOutputs",
            "preset",
            "advanced",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FileOutputs,
            SegmentOutputs,
            Preset,
            Advanced,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "fileOutputs" | "file_outputs" => Ok(GeneratedField::FileOutputs),
                            "segmentOutputs" | "segment_outputs" => Ok(GeneratedField::SegmentOutputs),
                            "preset" => Ok(GeneratedField::Preset),
                            "advanced" => Ok(GeneratedField::Advanced),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AutoParticipantEgress;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.AutoParticipantEgress")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<AutoParticipantEgress, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut file_outputs__ = None;
                let mut segment_outputs__ = None;
                let mut options__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::FileOutputs => {
                            if file_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fileOutputs"));
                            }
                            file_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::SegmentOutputs => {
                            if segment_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segmentOutputs"));
                            }
                            segment_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::Preset => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preset"));
                            }
                            options__ = map.next_value::<::std::option::Option<EncodingOptionsPreset>>()?.map(|x| auto_participant_egress::Options::Preset(x as i32));
                        }
                        GeneratedField::Advanced => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("advanced"));
                            }
                            options__ = map.next_value::<::std::option::Option<_>>()?.map(auto_participant_egress::Options::Advanced)
;
                        }
                    }
                }
                Ok(AutoParticipantEgress {
                    file_outputs: file_outputs__.unwrap_or_default(),
                    segment_outputs: segment_outputs__.unwrap_or_default(),
                    options: options__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.AutoParticipantEgress", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AutoTrackEgress {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.filepath.is_empty() {
            len += 1;
        }
        if self.disable_manifest {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.AutoTrackEgress", len)?;
        if !self.filepath.is_empty() {
            struct_ser.serialize_field("filepath", &self.filepath)?;
        }
        if self.disable_manifest {
            struct_ser.serialize_field("disableManifest", &self.disable_manifest)?;
        }
        if let Some(v) = self.output.as_ref() {
            match v {
                auto_track_egress::Output::S3(v) => {
                    struct_ser.serialize_field("s3", v)?;
                }
                auto_track_egress::Output::Gcp(v) => {
                    struct_ser.serialize_field("gcp", v)?;
                }
                auto_track_egress::Output::Azure(v) => {
                    struct_ser.serialize_field("azure", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AutoTrackEgress {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "filepath",
            "disable_manifest",
            "disableManifest",
            "s3",
            "gcp",
            "azure",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Filepath,
            DisableManifest,
            S3,
            Gcp,
            Azure,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "filepath" => Ok(GeneratedField::Filepath),
                            "disableManifest" | "disable_manifest" => Ok(GeneratedField::DisableManifest),
                            "s3" => Ok(GeneratedField::S3),
                            "gcp" => Ok(GeneratedField::Gcp),
                            "azure" => Ok(GeneratedField::Azure),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AutoTrackEgress;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.AutoTrackEgress")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<AutoTrackEgress, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut filepath__ = None;
                let mut disable_manifest__ = None;
                let mut output__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Filepath => {
                            if filepath__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filepath"));
                            }
                            filepath__ = Some(map.next_value()?);
                        }
                        GeneratedField::DisableManifest => {
                            if disable_manifest__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableManifest"));
                            }
                            disable_manifest__ = Some(map.next_value()?);
                        }
                        GeneratedField::S3 => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("s3"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(auto_track_egress::Output::S3)
;
                        }
                        GeneratedField::Gcp => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("gcp"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(auto_track_egress::Output::Gcp)
;
                        }
                        GeneratedField::Azure => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("azure"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(auto_track_egress::Output::Azure)
;
                        }
                    }
                }
                Ok(AutoTrackEgress {
                    filepath: filepath__.unwrap_or_default(),
                    disable_manifest: disable_manifest__.unwrap_or_default(),
                    output: output__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.AutoTrackEgress", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AzureBlobUpload {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.account_name.is_empty() {
            len += 1;
        }
        if !self.account_key.is_empty() {
            len += 1;
        }
        if !self.container_name.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.AzureBlobUpload", len)?;
        if !self.account_name.is_empty() {
            struct_ser.serialize_field("accountName", &self.account_name)?;
        }
        if !self.account_key.is_empty() {
            struct_ser.serialize_field("accountKey", &self.account_key)?;
        }
        if !self.container_name.is_empty() {
            struct_ser.serialize_field("containerName", &self.container_name)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AzureBlobUpload {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "account_name",
            "accountName",
            "account_key",
            "accountKey",
            "container_name",
            "containerName",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AccountName,
            AccountKey,
            ContainerName,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "accountName" | "account_name" => Ok(GeneratedField::AccountName),
                            "accountKey" | "account_key" => Ok(GeneratedField::AccountKey),
                            "containerName" | "container_name" => Ok(GeneratedField::ContainerName),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AzureBlobUpload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.AzureBlobUpload")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<AzureBlobUpload, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut account_name__ = None;
                let mut account_key__ = None;
                let mut container_name__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::AccountName => {
                            if account_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountName"));
                            }
                            account_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::AccountKey => {
                            if account_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountKey"));
                            }
                            account_key__ = Some(map.next_value()?);
                        }
                        GeneratedField::ContainerName => {
                            if container_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("containerName"));
                            }
                            container_name__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(AzureBlobUpload {
                    account_name: account_name__.unwrap_or_default(),
                    account_key: account_key__.unwrap_or_default(),
                    container_name: container_name__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.AzureBlobUpload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CandidateProtocol {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Udp => "UDP",
            Self::Tcp => "TCP",
            Self::Tls => "TLS",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for CandidateProtocol {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "UDP",
            "TCP",
            "TLS",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CandidateProtocol;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(CandidateProtocol::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(CandidateProtocol::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "UDP" => Ok(CandidateProtocol::Udp),
                    "TCP" => Ok(CandidateProtocol::Tcp),
                    "TLS" => Ok(CandidateProtocol::Tls),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ClientConfigSetting {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unset => "UNSET",
            Self::Disabled => "DISABLED",
            Self::Enabled => "ENABLED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ClientConfigSetting {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "UNSET",
            "DISABLED",
            "ENABLED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ClientConfigSetting;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ClientConfigSetting::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ClientConfigSetting::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "UNSET" => Ok(ClientConfigSetting::Unset),
                    "DISABLED" => Ok(ClientConfigSetting::Disabled),
                    "ENABLED" => Ok(ClientConfigSetting::Enabled),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ClientConfiguration {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.video.is_some() {
            len += 1;
        }
        if self.screen.is_some() {
            len += 1;
        }
        if self.resume_connection != 0 {
            len += 1;
        }
        if self.disabled_codecs.is_some() {
            len += 1;
        }
        if self.force_relay != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ClientConfiguration", len)?;
        if let Some(v) = self.video.as_ref() {
            struct_ser.serialize_field("video", v)?;
        }
        if let Some(v) = self.screen.as_ref() {
            struct_ser.serialize_field("screen", v)?;
        }
        if self.resume_connection != 0 {
            let v = ClientConfigSetting::from_i32(self.resume_connection)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.resume_connection)))?;
            struct_ser.serialize_field("resumeConnection", &v)?;
        }
        if let Some(v) = self.disabled_codecs.as_ref() {
            struct_ser.serialize_field("disabledCodecs", v)?;
        }
        if self.force_relay != 0 {
            let v = ClientConfigSetting::from_i32(self.force_relay)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.force_relay)))?;
            struct_ser.serialize_field("forceRelay", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ClientConfiguration {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "video",
            "screen",
            "resume_connection",
            "resumeConnection",
            "disabled_codecs",
            "disabledCodecs",
            "force_relay",
            "forceRelay",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Video,
            Screen,
            ResumeConnection,
            DisabledCodecs,
            ForceRelay,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "video" => Ok(GeneratedField::Video),
                            "screen" => Ok(GeneratedField::Screen),
                            "resumeConnection" | "resume_connection" => Ok(GeneratedField::ResumeConnection),
                            "disabledCodecs" | "disabled_codecs" => Ok(GeneratedField::DisabledCodecs),
                            "forceRelay" | "force_relay" => Ok(GeneratedField::ForceRelay),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ClientConfiguration;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ClientConfiguration")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ClientConfiguration, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut video__ = None;
                let mut screen__ = None;
                let mut resume_connection__ = None;
                let mut disabled_codecs__ = None;
                let mut force_relay__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Video => {
                            if video__.is_some() {
                                return Err(serde::de::Error::duplicate_field("video"));
                            }
                            video__ = map.next_value()?;
                        }
                        GeneratedField::Screen => {
                            if screen__.is_some() {
                                return Err(serde::de::Error::duplicate_field("screen"));
                            }
                            screen__ = map.next_value()?;
                        }
                        GeneratedField::ResumeConnection => {
                            if resume_connection__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resumeConnection"));
                            }
                            resume_connection__ = Some(map.next_value::<ClientConfigSetting>()? as i32);
                        }
                        GeneratedField::DisabledCodecs => {
                            if disabled_codecs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disabledCodecs"));
                            }
                            disabled_codecs__ = map.next_value()?;
                        }
                        GeneratedField::ForceRelay => {
                            if force_relay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("forceRelay"));
                            }
                            force_relay__ = Some(map.next_value::<ClientConfigSetting>()? as i32);
                        }
                    }
                }
                Ok(ClientConfiguration {
                    video: video__,
                    screen: screen__,
                    resume_connection: resume_connection__.unwrap_or_default(),
                    disabled_codecs: disabled_codecs__,
                    force_relay: force_relay__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ClientConfiguration", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ClientInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.sdk != 0 {
            len += 1;
        }
        if !self.version.is_empty() {
            len += 1;
        }
        if self.protocol != 0 {
            len += 1;
        }
        if !self.os.is_empty() {
            len += 1;
        }
        if !self.os_version.is_empty() {
            len += 1;
        }
        if !self.device_model.is_empty() {
            len += 1;
        }
        if !self.browser.is_empty() {
            len += 1;
        }
        if !self.browser_version.is_empty() {
            len += 1;
        }
        if !self.address.is_empty() {
            len += 1;
        }
        if !self.network.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ClientInfo", len)?;
        if self.sdk != 0 {
            let v = client_info::Sdk::from_i32(self.sdk)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.sdk)))?;
            struct_ser.serialize_field("sdk", &v)?;
        }
        if !self.version.is_empty() {
            struct_ser.serialize_field("version", &self.version)?;
        }
        if self.protocol != 0 {
            struct_ser.serialize_field("protocol", &self.protocol)?;
        }
        if !self.os.is_empty() {
            struct_ser.serialize_field("os", &self.os)?;
        }
        if !self.os_version.is_empty() {
            struct_ser.serialize_field("osVersion", &self.os_version)?;
        }
        if !self.device_model.is_empty() {
            struct_ser.serialize_field("deviceModel", &self.device_model)?;
        }
        if !self.browser.is_empty() {
            struct_ser.serialize_field("browser", &self.browser)?;
        }
        if !self.browser_version.is_empty() {
            struct_ser.serialize_field("browserVersion", &self.browser_version)?;
        }
        if !self.address.is_empty() {
            struct_ser.serialize_field("address", &self.address)?;
        }
        if !self.network.is_empty() {
            struct_ser.serialize_field("network", &self.network)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ClientInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sdk",
            "version",
            "protocol",
            "os",
            "os_version",
            "osVersion",
            "device_model",
            "deviceModel",
            "browser",
            "browser_version",
            "browserVersion",
            "address",
            "network",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Sdk,
            Version,
            Protocol,
            Os,
            OsVersion,
            DeviceModel,
            Browser,
            BrowserVersion,
            Address,
            Network,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "sdk" => Ok(GeneratedField::Sdk),
                            "version" => Ok(GeneratedField::Version),
                            "protocol" => Ok(GeneratedField::Protocol),
                            "os" => Ok(GeneratedField::Os),
                            "osVersion" | "os_version" => Ok(GeneratedField::OsVersion),
                            "deviceModel" | "device_model" => Ok(GeneratedField::DeviceModel),
                            "browser" => Ok(GeneratedField::Browser),
                            "browserVersion" | "browser_version" => Ok(GeneratedField::BrowserVersion),
                            "address" => Ok(GeneratedField::Address),
                            "network" => Ok(GeneratedField::Network),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ClientInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ClientInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ClientInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sdk__ = None;
                let mut version__ = None;
                let mut protocol__ = None;
                let mut os__ = None;
                let mut os_version__ = None;
                let mut device_model__ = None;
                let mut browser__ = None;
                let mut browser_version__ = None;
                let mut address__ = None;
                let mut network__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Sdk => {
                            if sdk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sdk"));
                            }
                            sdk__ = Some(map.next_value::<client_info::Sdk>()? as i32);
                        }
                        GeneratedField::Version => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("version"));
                            }
                            version__ = Some(map.next_value()?);
                        }
                        GeneratedField::Protocol => {
                            if protocol__.is_some() {
                                return Err(serde::de::Error::duplicate_field("protocol"));
                            }
                            protocol__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Os => {
                            if os__.is_some() {
                                return Err(serde::de::Error::duplicate_field("os"));
                            }
                            os__ = Some(map.next_value()?);
                        }
                        GeneratedField::OsVersion => {
                            if os_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("osVersion"));
                            }
                            os_version__ = Some(map.next_value()?);
                        }
                        GeneratedField::DeviceModel => {
                            if device_model__.is_some() {
                                return Err(serde::de::Error::duplicate_field("deviceModel"));
                            }
                            device_model__ = Some(map.next_value()?);
                        }
                        GeneratedField::Browser => {
                            if browser__.is_some() {
                                return Err(serde::de::Error::duplicate_field("browser"));
                            }
                            browser__ = Some(map.next_value()?);
                        }
                        GeneratedField::BrowserVersion => {
                            if browser_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("browserVersion"));
                            }
                            browser_version__ = Some(map.next_value()?);
                        }
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = Some(map.next_value()?);
                        }
                        GeneratedField::Network => {
                            if network__.is_some() {
                                return Err(serde::de::Error::duplicate_field("network"));
                            }
                            network__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ClientInfo {
                    sdk: sdk__.unwrap_or_default(),
                    version: version__.unwrap_or_default(),
                    protocol: protocol__.unwrap_or_default(),
                    os: os__.unwrap_or_default(),
                    os_version: os_version__.unwrap_or_default(),
                    device_model: device_model__.unwrap_or_default(),
                    browser: browser__.unwrap_or_default(),
                    browser_version: browser_version__.unwrap_or_default(),
                    address: address__.unwrap_or_default(),
                    network: network__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ClientInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for client_info::Sdk {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unknown => "UNKNOWN",
            Self::Js => "JS",
            Self::Swift => "SWIFT",
            Self::Android => "ANDROID",
            Self::Flutter => "FLUTTER",
            Self::Go => "GO",
            Self::Unity => "UNITY",
            Self::ReactNative => "REACT_NATIVE",
            Self::Rust => "RUST",
            Self::Python => "PYTHON",
            Self::Cpp => "CPP",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for client_info::Sdk {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "UNKNOWN",
            "JS",
            "SWIFT",
            "ANDROID",
            "FLUTTER",
            "GO",
            "UNITY",
            "REACT_NATIVE",
            "RUST",
            "PYTHON",
            "CPP",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = client_info::Sdk;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(client_info::Sdk::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(client_info::Sdk::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "UNKNOWN" => Ok(client_info::Sdk::Unknown),
                    "JS" => Ok(client_info::Sdk::Js),
                    "SWIFT" => Ok(client_info::Sdk::Swift),
                    "ANDROID" => Ok(client_info::Sdk::Android),
                    "FLUTTER" => Ok(client_info::Sdk::Flutter),
                    "GO" => Ok(client_info::Sdk::Go),
                    "UNITY" => Ok(client_info::Sdk::Unity),
                    "REACT_NATIVE" => Ok(client_info::Sdk::ReactNative),
                    "RUST" => Ok(client_info::Sdk::Rust),
                    "PYTHON" => Ok(client_info::Sdk::Python),
                    "CPP" => Ok(client_info::Sdk::Cpp),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for Codec {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.mime.is_empty() {
            len += 1;
        }
        if !self.fmtp_line.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.Codec", len)?;
        if !self.mime.is_empty() {
            struct_ser.serialize_field("mime", &self.mime)?;
        }
        if !self.fmtp_line.is_empty() {
            struct_ser.serialize_field("fmtpLine", &self.fmtp_line)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Codec {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "mime",
            "fmtp_line",
            "fmtpLine",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Mime,
            FmtpLine,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "mime" => Ok(GeneratedField::Mime),
                            "fmtpLine" | "fmtp_line" => Ok(GeneratedField::FmtpLine),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Codec;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.Codec")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<Codec, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut mime__ = None;
                let mut fmtp_line__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Mime => {
                            if mime__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mime"));
                            }
                            mime__ = Some(map.next_value()?);
                        }
                        GeneratedField::FmtpLine => {
                            if fmtp_line__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fmtpLine"));
                            }
                            fmtp_line__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(Codec {
                    mime: mime__.unwrap_or_default(),
                    fmtp_line: fmtp_line__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.Codec", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ConnectionQuality {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Poor => "POOR",
            Self::Good => "GOOD",
            Self::Excellent => "EXCELLENT",
            Self::Lost => "LOST",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ConnectionQuality {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "POOR",
            "GOOD",
            "EXCELLENT",
            "LOST",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ConnectionQuality;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ConnectionQuality::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ConnectionQuality::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "POOR" => Ok(ConnectionQuality::Poor),
                    "GOOD" => Ok(ConnectionQuality::Good),
                    "EXCELLENT" => Ok(ConnectionQuality::Excellent),
                    "LOST" => Ok(ConnectionQuality::Lost),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ConnectionQualityInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.participant_sid.is_empty() {
            len += 1;
        }
        if self.quality != 0 {
            len += 1;
        }
        if self.score != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ConnectionQualityInfo", len)?;
        if !self.participant_sid.is_empty() {
            struct_ser.serialize_field("participantSid", &self.participant_sid)?;
        }
        if self.quality != 0 {
            let v = ConnectionQuality::from_i32(self.quality)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.quality)))?;
            struct_ser.serialize_field("quality", &v)?;
        }
        if self.score != 0. {
            struct_ser.serialize_field("score", &self.score)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ConnectionQualityInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "participant_sid",
            "participantSid",
            "quality",
            "score",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ParticipantSid,
            Quality,
            Score,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "participantSid" | "participant_sid" => Ok(GeneratedField::ParticipantSid),
                            "quality" => Ok(GeneratedField::Quality),
                            "score" => Ok(GeneratedField::Score),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ConnectionQualityInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ConnectionQualityInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ConnectionQualityInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut participant_sid__ = None;
                let mut quality__ = None;
                let mut score__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::ParticipantSid => {
                            if participant_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantSid"));
                            }
                            participant_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Quality => {
                            if quality__.is_some() {
                                return Err(serde::de::Error::duplicate_field("quality"));
                            }
                            quality__ = Some(map.next_value::<ConnectionQuality>()? as i32);
                        }
                        GeneratedField::Score => {
                            if score__.is_some() {
                                return Err(serde::de::Error::duplicate_field("score"));
                            }
                            score__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(ConnectionQualityInfo {
                    participant_sid: participant_sid__.unwrap_or_default(),
                    quality: quality__.unwrap_or_default(),
                    score: score__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ConnectionQualityInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ConnectionQualityUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.updates.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ConnectionQualityUpdate", len)?;
        if !self.updates.is_empty() {
            struct_ser.serialize_field("updates", &self.updates)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ConnectionQualityUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "updates",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Updates,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "updates" => Ok(GeneratedField::Updates),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ConnectionQualityUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ConnectionQualityUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ConnectionQualityUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut updates__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Updates => {
                            if updates__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updates"));
                            }
                            updates__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ConnectionQualityUpdate {
                    updates: updates__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ConnectionQualityUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CreateIngressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.input_type != 0 {
            len += 1;
        }
        if !self.url.is_empty() {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        if !self.room_name.is_empty() {
            len += 1;
        }
        if !self.participant_identity.is_empty() {
            len += 1;
        }
        if !self.participant_name.is_empty() {
            len += 1;
        }
        if self.bypass_transcoding {
            len += 1;
        }
        if self.audio.is_some() {
            len += 1;
        }
        if self.video.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.CreateIngressRequest", len)?;
        if self.input_type != 0 {
            let v = IngressInput::from_i32(self.input_type)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.input_type)))?;
            struct_ser.serialize_field("inputType", &v)?;
        }
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if !self.participant_identity.is_empty() {
            struct_ser.serialize_field("participantIdentity", &self.participant_identity)?;
        }
        if !self.participant_name.is_empty() {
            struct_ser.serialize_field("participantName", &self.participant_name)?;
        }
        if self.bypass_transcoding {
            struct_ser.serialize_field("bypassTranscoding", &self.bypass_transcoding)?;
        }
        if let Some(v) = self.audio.as_ref() {
            struct_ser.serialize_field("audio", v)?;
        }
        if let Some(v) = self.video.as_ref() {
            struct_ser.serialize_field("video", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CreateIngressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "input_type",
            "inputType",
            "url",
            "name",
            "room_name",
            "roomName",
            "participant_identity",
            "participantIdentity",
            "participant_name",
            "participantName",
            "bypass_transcoding",
            "bypassTranscoding",
            "audio",
            "video",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InputType,
            Url,
            Name,
            RoomName,
            ParticipantIdentity,
            ParticipantName,
            BypassTranscoding,
            Audio,
            Video,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "inputType" | "input_type" => Ok(GeneratedField::InputType),
                            "url" => Ok(GeneratedField::Url),
                            "name" => Ok(GeneratedField::Name),
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "participantIdentity" | "participant_identity" => Ok(GeneratedField::ParticipantIdentity),
                            "participantName" | "participant_name" => Ok(GeneratedField::ParticipantName),
                            "bypassTranscoding" | "bypass_transcoding" => Ok(GeneratedField::BypassTranscoding),
                            "audio" => Ok(GeneratedField::Audio),
                            "video" => Ok(GeneratedField::Video),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CreateIngressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.CreateIngressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<CreateIngressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut input_type__ = None;
                let mut url__ = None;
                let mut name__ = None;
                let mut room_name__ = None;
                let mut participant_identity__ = None;
                let mut participant_name__ = None;
                let mut bypass_transcoding__ = None;
                let mut audio__ = None;
                let mut video__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InputType => {
                            if input_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputType"));
                            }
                            input_type__ = Some(map.next_value::<IngressInput>()? as i32);
                        }
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map.next_value()?);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantIdentity => {
                            if participant_identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantIdentity"));
                            }
                            participant_identity__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantName => {
                            if participant_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantName"));
                            }
                            participant_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::BypassTranscoding => {
                            if bypass_transcoding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bypassTranscoding"));
                            }
                            bypass_transcoding__ = Some(map.next_value()?);
                        }
                        GeneratedField::Audio => {
                            if audio__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audio"));
                            }
                            audio__ = map.next_value()?;
                        }
                        GeneratedField::Video => {
                            if video__.is_some() {
                                return Err(serde::de::Error::duplicate_field("video"));
                            }
                            video__ = map.next_value()?;
                        }
                    }
                }
                Ok(CreateIngressRequest {
                    input_type: input_type__.unwrap_or_default(),
                    url: url__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                    room_name: room_name__.unwrap_or_default(),
                    participant_identity: participant_identity__.unwrap_or_default(),
                    participant_name: participant_name__.unwrap_or_default(),
                    bypass_transcoding: bypass_transcoding__.unwrap_or_default(),
                    audio: audio__,
                    video: video__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.CreateIngressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CreateRoomRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.name.is_empty() {
            len += 1;
        }
        if self.empty_timeout != 0 {
            len += 1;
        }
        if self.max_participants != 0 {
            len += 1;
        }
        if !self.node_id.is_empty() {
            len += 1;
        }
        if !self.metadata.is_empty() {
            len += 1;
        }
        if self.egress.is_some() {
            len += 1;
        }
        if self.min_playout_delay != 0 {
            len += 1;
        }
        if self.max_playout_delay != 0 {
            len += 1;
        }
        if self.sync_streams {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.CreateRoomRequest", len)?;
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if self.empty_timeout != 0 {
            struct_ser.serialize_field("emptyTimeout", &self.empty_timeout)?;
        }
        if self.max_participants != 0 {
            struct_ser.serialize_field("maxParticipants", &self.max_participants)?;
        }
        if !self.node_id.is_empty() {
            struct_ser.serialize_field("nodeId", &self.node_id)?;
        }
        if !self.metadata.is_empty() {
            struct_ser.serialize_field("metadata", &self.metadata)?;
        }
        if let Some(v) = self.egress.as_ref() {
            struct_ser.serialize_field("egress", v)?;
        }
        if self.min_playout_delay != 0 {
            struct_ser.serialize_field("minPlayoutDelay", &self.min_playout_delay)?;
        }
        if self.max_playout_delay != 0 {
            struct_ser.serialize_field("maxPlayoutDelay", &self.max_playout_delay)?;
        }
        if self.sync_streams {
            struct_ser.serialize_field("syncStreams", &self.sync_streams)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CreateRoomRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "name",
            "empty_timeout",
            "emptyTimeout",
            "max_participants",
            "maxParticipants",
            "node_id",
            "nodeId",
            "metadata",
            "egress",
            "min_playout_delay",
            "minPlayoutDelay",
            "max_playout_delay",
            "maxPlayoutDelay",
            "sync_streams",
            "syncStreams",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Name,
            EmptyTimeout,
            MaxParticipants,
            NodeId,
            Metadata,
            Egress,
            MinPlayoutDelay,
            MaxPlayoutDelay,
            SyncStreams,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "name" => Ok(GeneratedField::Name),
                            "emptyTimeout" | "empty_timeout" => Ok(GeneratedField::EmptyTimeout),
                            "maxParticipants" | "max_participants" => Ok(GeneratedField::MaxParticipants),
                            "nodeId" | "node_id" => Ok(GeneratedField::NodeId),
                            "metadata" => Ok(GeneratedField::Metadata),
                            "egress" => Ok(GeneratedField::Egress),
                            "minPlayoutDelay" | "min_playout_delay" => Ok(GeneratedField::MinPlayoutDelay),
                            "maxPlayoutDelay" | "max_playout_delay" => Ok(GeneratedField::MaxPlayoutDelay),
                            "syncStreams" | "sync_streams" => Ok(GeneratedField::SyncStreams),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CreateRoomRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.CreateRoomRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<CreateRoomRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut name__ = None;
                let mut empty_timeout__ = None;
                let mut max_participants__ = None;
                let mut node_id__ = None;
                let mut metadata__ = None;
                let mut egress__ = None;
                let mut min_playout_delay__ = None;
                let mut max_playout_delay__ = None;
                let mut sync_streams__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::EmptyTimeout => {
                            if empty_timeout__.is_some() {
                                return Err(serde::de::Error::duplicate_field("emptyTimeout"));
                            }
                            empty_timeout__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MaxParticipants => {
                            if max_participants__.is_some() {
                                return Err(serde::de::Error::duplicate_field("maxParticipants"));
                            }
                            max_participants__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NodeId => {
                            if node_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nodeId"));
                            }
                            node_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = Some(map.next_value()?);
                        }
                        GeneratedField::Egress => {
                            if egress__.is_some() {
                                return Err(serde::de::Error::duplicate_field("egress"));
                            }
                            egress__ = map.next_value()?;
                        }
                        GeneratedField::MinPlayoutDelay => {
                            if min_playout_delay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("minPlayoutDelay"));
                            }
                            min_playout_delay__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MaxPlayoutDelay => {
                            if max_playout_delay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("maxPlayoutDelay"));
                            }
                            max_playout_delay__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SyncStreams => {
                            if sync_streams__.is_some() {
                                return Err(serde::de::Error::duplicate_field("syncStreams"));
                            }
                            sync_streams__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(CreateRoomRequest {
                    name: name__.unwrap_or_default(),
                    empty_timeout: empty_timeout__.unwrap_or_default(),
                    max_participants: max_participants__.unwrap_or_default(),
                    node_id: node_id__.unwrap_or_default(),
                    metadata: metadata__.unwrap_or_default(),
                    egress: egress__,
                    min_playout_delay: min_playout_delay__.unwrap_or_default(),
                    max_playout_delay: max_playout_delay__.unwrap_or_default(),
                    sync_streams: sync_streams__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.CreateRoomRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DataChannelInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.label.is_empty() {
            len += 1;
        }
        if self.id != 0 {
            len += 1;
        }
        if self.target != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.DataChannelInfo", len)?;
        if !self.label.is_empty() {
            struct_ser.serialize_field("label", &self.label)?;
        }
        if self.id != 0 {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if self.target != 0 {
            let v = SignalTarget::from_i32(self.target)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.target)))?;
            struct_ser.serialize_field("target", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DataChannelInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "label",
            "id",
            "target",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Label,
            Id,
            Target,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "label" => Ok(GeneratedField::Label),
                            "id" => Ok(GeneratedField::Id),
                            "target" => Ok(GeneratedField::Target),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DataChannelInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.DataChannelInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DataChannelInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut label__ = None;
                let mut id__ = None;
                let mut target__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Label => {
                            if label__.is_some() {
                                return Err(serde::de::Error::duplicate_field("label"));
                            }
                            label__ = Some(map.next_value()?);
                        }
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Target => {
                            if target__.is_some() {
                                return Err(serde::de::Error::duplicate_field("target"));
                            }
                            target__ = Some(map.next_value::<SignalTarget>()? as i32);
                        }
                    }
                }
                Ok(DataChannelInfo {
                    label: label__.unwrap_or_default(),
                    id: id__.unwrap_or_default(),
                    target: target__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.DataChannelInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DataPacket {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.kind != 0 {
            len += 1;
        }
        if self.value.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.DataPacket", len)?;
        if self.kind != 0 {
            let v = data_packet::Kind::from_i32(self.kind)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.kind)))?;
            struct_ser.serialize_field("kind", &v)?;
        }
        if let Some(v) = self.value.as_ref() {
            match v {
                data_packet::Value::User(v) => {
                    struct_ser.serialize_field("user", v)?;
                }
                data_packet::Value::Speaker(v) => {
                    struct_ser.serialize_field("speaker", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DataPacket {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "kind",
            "user",
            "speaker",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Kind,
            User,
            Speaker,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "kind" => Ok(GeneratedField::Kind),
                            "user" => Ok(GeneratedField::User),
                            "speaker" => Ok(GeneratedField::Speaker),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DataPacket;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.DataPacket")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DataPacket, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut kind__ = None;
                let mut value__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Kind => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("kind"));
                            }
                            kind__ = Some(map.next_value::<data_packet::Kind>()? as i32);
                        }
                        GeneratedField::User => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("user"));
                            }
                            value__ = map.next_value::<::std::option::Option<_>>()?.map(data_packet::Value::User)
;
                        }
                        GeneratedField::Speaker => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("speaker"));
                            }
                            value__ = map.next_value::<::std::option::Option<_>>()?.map(data_packet::Value::Speaker)
;
                        }
                    }
                }
                Ok(DataPacket {
                    kind: kind__.unwrap_or_default(),
                    value: value__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.DataPacket", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for data_packet::Kind {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Reliable => "RELIABLE",
            Self::Lossy => "LOSSY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for data_packet::Kind {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RELIABLE",
            "LOSSY",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = data_packet::Kind;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(data_packet::Kind::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(data_packet::Kind::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RELIABLE" => Ok(data_packet::Kind::Reliable),
                    "LOSSY" => Ok(data_packet::Kind::Lossy),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for DeleteIngressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.ingress_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.DeleteIngressRequest", len)?;
        if !self.ingress_id.is_empty() {
            struct_ser.serialize_field("ingressId", &self.ingress_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeleteIngressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ingress_id",
            "ingressId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IngressId,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "ingressId" | "ingress_id" => Ok(GeneratedField::IngressId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeleteIngressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.DeleteIngressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DeleteIngressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut ingress_id__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IngressId => {
                            if ingress_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ingressId"));
                            }
                            ingress_id__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(DeleteIngressRequest {
                    ingress_id: ingress_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.DeleteIngressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DeleteRoomRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.DeleteRoomRequest", len)?;
        if !self.room.is_empty() {
            struct_ser.serialize_field("room", &self.room)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeleteRoomRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeleteRoomRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.DeleteRoomRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DeleteRoomRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(DeleteRoomRequest {
                    room: room__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.DeleteRoomRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DeleteRoomResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.DeleteRoomResponse", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeleteRoomResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Err(serde::de::Error::unknown_field(value, FIELDS))
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeleteRoomResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.DeleteRoomResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DeleteRoomResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map.next_key::<GeneratedField>()?.is_some() {
                    let _ = map.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(DeleteRoomResponse {
                })
            }
        }
        deserializer.deserialize_struct("livekit.DeleteRoomResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DirectFileOutput {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.filepath.is_empty() {
            len += 1;
        }
        if self.disable_manifest {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.DirectFileOutput", len)?;
        if !self.filepath.is_empty() {
            struct_ser.serialize_field("filepath", &self.filepath)?;
        }
        if self.disable_manifest {
            struct_ser.serialize_field("disableManifest", &self.disable_manifest)?;
        }
        if let Some(v) = self.output.as_ref() {
            match v {
                direct_file_output::Output::S3(v) => {
                    struct_ser.serialize_field("s3", v)?;
                }
                direct_file_output::Output::Gcp(v) => {
                    struct_ser.serialize_field("gcp", v)?;
                }
                direct_file_output::Output::Azure(v) => {
                    struct_ser.serialize_field("azure", v)?;
                }
                direct_file_output::Output::AliOss(v) => {
                    struct_ser.serialize_field("aliOSS", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DirectFileOutput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "filepath",
            "disable_manifest",
            "disableManifest",
            "s3",
            "gcp",
            "azure",
            "aliOSS",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Filepath,
            DisableManifest,
            S3,
            Gcp,
            Azure,
            AliOss,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "filepath" => Ok(GeneratedField::Filepath),
                            "disableManifest" | "disable_manifest" => Ok(GeneratedField::DisableManifest),
                            "s3" => Ok(GeneratedField::S3),
                            "gcp" => Ok(GeneratedField::Gcp),
                            "azure" => Ok(GeneratedField::Azure),
                            "aliOSS" => Ok(GeneratedField::AliOss),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DirectFileOutput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.DirectFileOutput")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DirectFileOutput, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut filepath__ = None;
                let mut disable_manifest__ = None;
                let mut output__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Filepath => {
                            if filepath__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filepath"));
                            }
                            filepath__ = Some(map.next_value()?);
                        }
                        GeneratedField::DisableManifest => {
                            if disable_manifest__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableManifest"));
                            }
                            disable_manifest__ = Some(map.next_value()?);
                        }
                        GeneratedField::S3 => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("s3"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(direct_file_output::Output::S3)
;
                        }
                        GeneratedField::Gcp => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("gcp"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(direct_file_output::Output::Gcp)
;
                        }
                        GeneratedField::Azure => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("azure"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(direct_file_output::Output::Azure)
;
                        }
                        GeneratedField::AliOss => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("aliOSS"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(direct_file_output::Output::AliOss)
;
                        }
                    }
                }
                Ok(DirectFileOutput {
                    filepath: filepath__.unwrap_or_default(),
                    disable_manifest: disable_manifest__.unwrap_or_default(),
                    output: output__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.DirectFileOutput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DisabledCodecs {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.codecs.is_empty() {
            len += 1;
        }
        if !self.publish.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.DisabledCodecs", len)?;
        if !self.codecs.is_empty() {
            struct_ser.serialize_field("codecs", &self.codecs)?;
        }
        if !self.publish.is_empty() {
            struct_ser.serialize_field("publish", &self.publish)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DisabledCodecs {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "codecs",
            "publish",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Codecs,
            Publish,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "codecs" => Ok(GeneratedField::Codecs),
                            "publish" => Ok(GeneratedField::Publish),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DisabledCodecs;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.DisabledCodecs")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DisabledCodecs, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut codecs__ = None;
                let mut publish__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Codecs => {
                            if codecs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("codecs"));
                            }
                            codecs__ = Some(map.next_value()?);
                        }
                        GeneratedField::Publish => {
                            if publish__.is_some() {
                                return Err(serde::de::Error::duplicate_field("publish"));
                            }
                            publish__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(DisabledCodecs {
                    codecs: codecs__.unwrap_or_default(),
                    publish: publish__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.DisabledCodecs", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DisconnectReason {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::UnknownReason => "UNKNOWN_REASON",
            Self::ClientInitiated => "CLIENT_INITIATED",
            Self::DuplicateIdentity => "DUPLICATE_IDENTITY",
            Self::ServerShutdown => "SERVER_SHUTDOWN",
            Self::ParticipantRemoved => "PARTICIPANT_REMOVED",
            Self::RoomDeleted => "ROOM_DELETED",
            Self::StateMismatch => "STATE_MISMATCH",
            Self::JoinFailure => "JOIN_FAILURE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for DisconnectReason {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "UNKNOWN_REASON",
            "CLIENT_INITIATED",
            "DUPLICATE_IDENTITY",
            "SERVER_SHUTDOWN",
            "PARTICIPANT_REMOVED",
            "ROOM_DELETED",
            "STATE_MISMATCH",
            "JOIN_FAILURE",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DisconnectReason;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(DisconnectReason::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(DisconnectReason::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "UNKNOWN_REASON" => Ok(DisconnectReason::UnknownReason),
                    "CLIENT_INITIATED" => Ok(DisconnectReason::ClientInitiated),
                    "DUPLICATE_IDENTITY" => Ok(DisconnectReason::DuplicateIdentity),
                    "SERVER_SHUTDOWN" => Ok(DisconnectReason::ServerShutdown),
                    "PARTICIPANT_REMOVED" => Ok(DisconnectReason::ParticipantRemoved),
                    "ROOM_DELETED" => Ok(DisconnectReason::RoomDeleted),
                    "STATE_MISMATCH" => Ok(DisconnectReason::StateMismatch),
                    "JOIN_FAILURE" => Ok(DisconnectReason::JoinFailure),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for EgressInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.egress_id.is_empty() {
            len += 1;
        }
        if !self.room_id.is_empty() {
            len += 1;
        }
        if !self.room_name.is_empty() {
            len += 1;
        }
        if self.status != 0 {
            len += 1;
        }
        if self.started_at != 0 {
            len += 1;
        }
        if self.ended_at != 0 {
            len += 1;
        }
        if self.updated_at != 0 {
            len += 1;
        }
        if !self.error.is_empty() {
            len += 1;
        }
        if !self.stream_results.is_empty() {
            len += 1;
        }
        if !self.file_results.is_empty() {
            len += 1;
        }
        if !self.segment_results.is_empty() {
            len += 1;
        }
        if !self.image_results.is_empty() {
            len += 1;
        }
        if self.request.is_some() {
            len += 1;
        }
        if self.result.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.EgressInfo", len)?;
        if !self.egress_id.is_empty() {
            struct_ser.serialize_field("egressId", &self.egress_id)?;
        }
        if !self.room_id.is_empty() {
            struct_ser.serialize_field("roomId", &self.room_id)?;
        }
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if self.status != 0 {
            let v = EgressStatus::from_i32(self.status)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.status)))?;
            struct_ser.serialize_field("status", &v)?;
        }
        if self.started_at != 0 {
            struct_ser.serialize_field("startedAt", ToString::to_string(&self.started_at).as_str())?;
        }
        if self.ended_at != 0 {
            struct_ser.serialize_field("endedAt", ToString::to_string(&self.ended_at).as_str())?;
        }
        if self.updated_at != 0 {
            struct_ser.serialize_field("updatedAt", ToString::to_string(&self.updated_at).as_str())?;
        }
        if !self.error.is_empty() {
            struct_ser.serialize_field("error", &self.error)?;
        }
        if !self.stream_results.is_empty() {
            struct_ser.serialize_field("streamResults", &self.stream_results)?;
        }
        if !self.file_results.is_empty() {
            struct_ser.serialize_field("fileResults", &self.file_results)?;
        }
        if !self.segment_results.is_empty() {
            struct_ser.serialize_field("segmentResults", &self.segment_results)?;
        }
        if !self.image_results.is_empty() {
            struct_ser.serialize_field("imageResults", &self.image_results)?;
        }
        if let Some(v) = self.request.as_ref() {
            match v {
                egress_info::Request::RoomComposite(v) => {
                    struct_ser.serialize_field("roomComposite", v)?;
                }
                egress_info::Request::Web(v) => {
                    struct_ser.serialize_field("web", v)?;
                }
                egress_info::Request::Participant(v) => {
                    struct_ser.serialize_field("participant", v)?;
                }
                egress_info::Request::TrackComposite(v) => {
                    struct_ser.serialize_field("trackComposite", v)?;
                }
                egress_info::Request::Track(v) => {
                    struct_ser.serialize_field("track", v)?;
                }
            }
        }
        if let Some(v) = self.result.as_ref() {
            match v {
                egress_info::Result::Stream(v) => {
                    struct_ser.serialize_field("stream", v)?;
                }
                egress_info::Result::File(v) => {
                    struct_ser.serialize_field("file", v)?;
                }
                egress_info::Result::Segments(v) => {
                    struct_ser.serialize_field("segments", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EgressInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "egress_id",
            "egressId",
            "room_id",
            "roomId",
            "room_name",
            "roomName",
            "status",
            "started_at",
            "startedAt",
            "ended_at",
            "endedAt",
            "updated_at",
            "updatedAt",
            "error",
            "stream_results",
            "streamResults",
            "file_results",
            "fileResults",
            "segment_results",
            "segmentResults",
            "image_results",
            "imageResults",
            "room_composite",
            "roomComposite",
            "web",
            "participant",
            "track_composite",
            "trackComposite",
            "track",
            "stream",
            "file",
            "segments",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EgressId,
            RoomId,
            RoomName,
            Status,
            StartedAt,
            EndedAt,
            UpdatedAt,
            Error,
            StreamResults,
            FileResults,
            SegmentResults,
            ImageResults,
            RoomComposite,
            Web,
            Participant,
            TrackComposite,
            Track,
            Stream,
            File,
            Segments,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "egressId" | "egress_id" => Ok(GeneratedField::EgressId),
                            "roomId" | "room_id" => Ok(GeneratedField::RoomId),
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "status" => Ok(GeneratedField::Status),
                            "startedAt" | "started_at" => Ok(GeneratedField::StartedAt),
                            "endedAt" | "ended_at" => Ok(GeneratedField::EndedAt),
                            "updatedAt" | "updated_at" => Ok(GeneratedField::UpdatedAt),
                            "error" => Ok(GeneratedField::Error),
                            "streamResults" | "stream_results" => Ok(GeneratedField::StreamResults),
                            "fileResults" | "file_results" => Ok(GeneratedField::FileResults),
                            "segmentResults" | "segment_results" => Ok(GeneratedField::SegmentResults),
                            "imageResults" | "image_results" => Ok(GeneratedField::ImageResults),
                            "roomComposite" | "room_composite" => Ok(GeneratedField::RoomComposite),
                            "web" => Ok(GeneratedField::Web),
                            "participant" => Ok(GeneratedField::Participant),
                            "trackComposite" | "track_composite" => Ok(GeneratedField::TrackComposite),
                            "track" => Ok(GeneratedField::Track),
                            "stream" => Ok(GeneratedField::Stream),
                            "file" => Ok(GeneratedField::File),
                            "segments" => Ok(GeneratedField::Segments),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EgressInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.EgressInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<EgressInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut egress_id__ = None;
                let mut room_id__ = None;
                let mut room_name__ = None;
                let mut status__ = None;
                let mut started_at__ = None;
                let mut ended_at__ = None;
                let mut updated_at__ = None;
                let mut error__ = None;
                let mut stream_results__ = None;
                let mut file_results__ = None;
                let mut segment_results__ = None;
                let mut image_results__ = None;
                let mut request__ = None;
                let mut result__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::EgressId => {
                            if egress_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("egressId"));
                            }
                            egress_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::RoomId => {
                            if room_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomId"));
                            }
                            room_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Status => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("status"));
                            }
                            status__ = Some(map.next_value::<EgressStatus>()? as i32);
                        }
                        GeneratedField::StartedAt => {
                            if started_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startedAt"));
                            }
                            started_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndedAt => {
                            if ended_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endedAt"));
                            }
                            ended_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::UpdatedAt => {
                            if updated_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updatedAt"));
                            }
                            updated_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Error => {
                            if error__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            error__ = Some(map.next_value()?);
                        }
                        GeneratedField::StreamResults => {
                            if stream_results__.is_some() {
                                return Err(serde::de::Error::duplicate_field("streamResults"));
                            }
                            stream_results__ = Some(map.next_value()?);
                        }
                        GeneratedField::FileResults => {
                            if file_results__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fileResults"));
                            }
                            file_results__ = Some(map.next_value()?);
                        }
                        GeneratedField::SegmentResults => {
                            if segment_results__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segmentResults"));
                            }
                            segment_results__ = Some(map.next_value()?);
                        }
                        GeneratedField::ImageResults => {
                            if image_results__.is_some() {
                                return Err(serde::de::Error::duplicate_field("imageResults"));
                            }
                            image_results__ = Some(map.next_value()?);
                        }
                        GeneratedField::RoomComposite => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomComposite"));
                            }
                            request__ = map.next_value::<::std::option::Option<_>>()?.map(egress_info::Request::RoomComposite)
;
                        }
                        GeneratedField::Web => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("web"));
                            }
                            request__ = map.next_value::<::std::option::Option<_>>()?.map(egress_info::Request::Web)
;
                        }
                        GeneratedField::Participant => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participant"));
                            }
                            request__ = map.next_value::<::std::option::Option<_>>()?.map(egress_info::Request::Participant)
;
                        }
                        GeneratedField::TrackComposite => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackComposite"));
                            }
                            request__ = map.next_value::<::std::option::Option<_>>()?.map(egress_info::Request::TrackComposite)
;
                        }
                        GeneratedField::Track => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("track"));
                            }
                            request__ = map.next_value::<::std::option::Option<_>>()?.map(egress_info::Request::Track)
;
                        }
                        GeneratedField::Stream => {
                            if result__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stream"));
                            }
                            result__ = map.next_value::<::std::option::Option<_>>()?.map(egress_info::Result::Stream)
;
                        }
                        GeneratedField::File => {
                            if result__.is_some() {
                                return Err(serde::de::Error::duplicate_field("file"));
                            }
                            result__ = map.next_value::<::std::option::Option<_>>()?.map(egress_info::Result::File)
;
                        }
                        GeneratedField::Segments => {
                            if result__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segments"));
                            }
                            result__ = map.next_value::<::std::option::Option<_>>()?.map(egress_info::Result::Segments)
;
                        }
                    }
                }
                Ok(EgressInfo {
                    egress_id: egress_id__.unwrap_or_default(),
                    room_id: room_id__.unwrap_or_default(),
                    room_name: room_name__.unwrap_or_default(),
                    status: status__.unwrap_or_default(),
                    started_at: started_at__.unwrap_or_default(),
                    ended_at: ended_at__.unwrap_or_default(),
                    updated_at: updated_at__.unwrap_or_default(),
                    error: error__.unwrap_or_default(),
                    stream_results: stream_results__.unwrap_or_default(),
                    file_results: file_results__.unwrap_or_default(),
                    segment_results: segment_results__.unwrap_or_default(),
                    image_results: image_results__.unwrap_or_default(),
                    request: request__,
                    result: result__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.EgressInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EgressStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::EgressStarting => "EGRESS_STARTING",
            Self::EgressActive => "EGRESS_ACTIVE",
            Self::EgressEnding => "EGRESS_ENDING",
            Self::EgressComplete => "EGRESS_COMPLETE",
            Self::EgressFailed => "EGRESS_FAILED",
            Self::EgressAborted => "EGRESS_ABORTED",
            Self::EgressLimitReached => "EGRESS_LIMIT_REACHED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for EgressStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "EGRESS_STARTING",
            "EGRESS_ACTIVE",
            "EGRESS_ENDING",
            "EGRESS_COMPLETE",
            "EGRESS_FAILED",
            "EGRESS_ABORTED",
            "EGRESS_LIMIT_REACHED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EgressStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(EgressStatus::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(EgressStatus::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "EGRESS_STARTING" => Ok(EgressStatus::EgressStarting),
                    "EGRESS_ACTIVE" => Ok(EgressStatus::EgressActive),
                    "EGRESS_ENDING" => Ok(EgressStatus::EgressEnding),
                    "EGRESS_COMPLETE" => Ok(EgressStatus::EgressComplete),
                    "EGRESS_FAILED" => Ok(EgressStatus::EgressFailed),
                    "EGRESS_ABORTED" => Ok(EgressStatus::EgressAborted),
                    "EGRESS_LIMIT_REACHED" => Ok(EgressStatus::EgressLimitReached),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for EncodedFileOutput {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.file_type != 0 {
            len += 1;
        }
        if !self.filepath.is_empty() {
            len += 1;
        }
        if self.disable_manifest {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.EncodedFileOutput", len)?;
        if self.file_type != 0 {
            let v = EncodedFileType::from_i32(self.file_type)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.file_type)))?;
            struct_ser.serialize_field("fileType", &v)?;
        }
        if !self.filepath.is_empty() {
            struct_ser.serialize_field("filepath", &self.filepath)?;
        }
        if self.disable_manifest {
            struct_ser.serialize_field("disableManifest", &self.disable_manifest)?;
        }
        if let Some(v) = self.output.as_ref() {
            match v {
                encoded_file_output::Output::S3(v) => {
                    struct_ser.serialize_field("s3", v)?;
                }
                encoded_file_output::Output::Gcp(v) => {
                    struct_ser.serialize_field("gcp", v)?;
                }
                encoded_file_output::Output::Azure(v) => {
                    struct_ser.serialize_field("azure", v)?;
                }
                encoded_file_output::Output::AliOss(v) => {
                    struct_ser.serialize_field("aliOSS", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EncodedFileOutput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "file_type",
            "fileType",
            "filepath",
            "disable_manifest",
            "disableManifest",
            "s3",
            "gcp",
            "azure",
            "aliOSS",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FileType,
            Filepath,
            DisableManifest,
            S3,
            Gcp,
            Azure,
            AliOss,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "fileType" | "file_type" => Ok(GeneratedField::FileType),
                            "filepath" => Ok(GeneratedField::Filepath),
                            "disableManifest" | "disable_manifest" => Ok(GeneratedField::DisableManifest),
                            "s3" => Ok(GeneratedField::S3),
                            "gcp" => Ok(GeneratedField::Gcp),
                            "azure" => Ok(GeneratedField::Azure),
                            "aliOSS" => Ok(GeneratedField::AliOss),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EncodedFileOutput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.EncodedFileOutput")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<EncodedFileOutput, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut file_type__ = None;
                let mut filepath__ = None;
                let mut disable_manifest__ = None;
                let mut output__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::FileType => {
                            if file_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fileType"));
                            }
                            file_type__ = Some(map.next_value::<EncodedFileType>()? as i32);
                        }
                        GeneratedField::Filepath => {
                            if filepath__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filepath"));
                            }
                            filepath__ = Some(map.next_value()?);
                        }
                        GeneratedField::DisableManifest => {
                            if disable_manifest__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableManifest"));
                            }
                            disable_manifest__ = Some(map.next_value()?);
                        }
                        GeneratedField::S3 => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("s3"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(encoded_file_output::Output::S3)
;
                        }
                        GeneratedField::Gcp => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("gcp"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(encoded_file_output::Output::Gcp)
;
                        }
                        GeneratedField::Azure => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("azure"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(encoded_file_output::Output::Azure)
;
                        }
                        GeneratedField::AliOss => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("aliOSS"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(encoded_file_output::Output::AliOss)
;
                        }
                    }
                }
                Ok(EncodedFileOutput {
                    file_type: file_type__.unwrap_or_default(),
                    filepath: filepath__.unwrap_or_default(),
                    disable_manifest: disable_manifest__.unwrap_or_default(),
                    output: output__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.EncodedFileOutput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EncodedFileType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::DefaultFiletype => "DEFAULT_FILETYPE",
            Self::Mp4 => "MP4",
            Self::Ogg => "OGG",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for EncodedFileType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "DEFAULT_FILETYPE",
            "MP4",
            "OGG",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EncodedFileType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(EncodedFileType::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(EncodedFileType::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "DEFAULT_FILETYPE" => Ok(EncodedFileType::DefaultFiletype),
                    "MP4" => Ok(EncodedFileType::Mp4),
                    "OGG" => Ok(EncodedFileType::Ogg),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for EncodingOptions {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.width != 0 {
            len += 1;
        }
        if self.height != 0 {
            len += 1;
        }
        if self.depth != 0 {
            len += 1;
        }
        if self.framerate != 0 {
            len += 1;
        }
        if self.audio_codec != 0 {
            len += 1;
        }
        if self.audio_bitrate != 0 {
            len += 1;
        }
        if self.audio_quality != 0 {
            len += 1;
        }
        if self.audio_frequency != 0 {
            len += 1;
        }
        if self.video_codec != 0 {
            len += 1;
        }
        if self.video_bitrate != 0 {
            len += 1;
        }
        if self.video_quality != 0 {
            len += 1;
        }
        if self.key_frame_interval != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.EncodingOptions", len)?;
        if self.width != 0 {
            struct_ser.serialize_field("width", &self.width)?;
        }
        if self.height != 0 {
            struct_ser.serialize_field("height", &self.height)?;
        }
        if self.depth != 0 {
            struct_ser.serialize_field("depth", &self.depth)?;
        }
        if self.framerate != 0 {
            struct_ser.serialize_field("framerate", &self.framerate)?;
        }
        if self.audio_codec != 0 {
            let v = AudioCodec::from_i32(self.audio_codec)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.audio_codec)))?;
            struct_ser.serialize_field("audioCodec", &v)?;
        }
        if self.audio_bitrate != 0 {
            struct_ser.serialize_field("audioBitrate", &self.audio_bitrate)?;
        }
        if self.audio_quality != 0 {
            struct_ser.serialize_field("audioQuality", &self.audio_quality)?;
        }
        if self.audio_frequency != 0 {
            struct_ser.serialize_field("audioFrequency", &self.audio_frequency)?;
        }
        if self.video_codec != 0 {
            let v = VideoCodec::from_i32(self.video_codec)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.video_codec)))?;
            struct_ser.serialize_field("videoCodec", &v)?;
        }
        if self.video_bitrate != 0 {
            struct_ser.serialize_field("videoBitrate", &self.video_bitrate)?;
        }
        if self.video_quality != 0 {
            struct_ser.serialize_field("videoQuality", &self.video_quality)?;
        }
        if self.key_frame_interval != 0. {
            struct_ser.serialize_field("keyFrameInterval", &self.key_frame_interval)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EncodingOptions {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "width",
            "height",
            "depth",
            "framerate",
            "audio_codec",
            "audioCodec",
            "audio_bitrate",
            "audioBitrate",
            "audio_quality",
            "audioQuality",
            "audio_frequency",
            "audioFrequency",
            "video_codec",
            "videoCodec",
            "video_bitrate",
            "videoBitrate",
            "video_quality",
            "videoQuality",
            "key_frame_interval",
            "keyFrameInterval",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Width,
            Height,
            Depth,
            Framerate,
            AudioCodec,
            AudioBitrate,
            AudioQuality,
            AudioFrequency,
            VideoCodec,
            VideoBitrate,
            VideoQuality,
            KeyFrameInterval,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "width" => Ok(GeneratedField::Width),
                            "height" => Ok(GeneratedField::Height),
                            "depth" => Ok(GeneratedField::Depth),
                            "framerate" => Ok(GeneratedField::Framerate),
                            "audioCodec" | "audio_codec" => Ok(GeneratedField::AudioCodec),
                            "audioBitrate" | "audio_bitrate" => Ok(GeneratedField::AudioBitrate),
                            "audioQuality" | "audio_quality" => Ok(GeneratedField::AudioQuality),
                            "audioFrequency" | "audio_frequency" => Ok(GeneratedField::AudioFrequency),
                            "videoCodec" | "video_codec" => Ok(GeneratedField::VideoCodec),
                            "videoBitrate" | "video_bitrate" => Ok(GeneratedField::VideoBitrate),
                            "videoQuality" | "video_quality" => Ok(GeneratedField::VideoQuality),
                            "keyFrameInterval" | "key_frame_interval" => Ok(GeneratedField::KeyFrameInterval),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EncodingOptions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.EncodingOptions")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<EncodingOptions, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut width__ = None;
                let mut height__ = None;
                let mut depth__ = None;
                let mut framerate__ = None;
                let mut audio_codec__ = None;
                let mut audio_bitrate__ = None;
                let mut audio_quality__ = None;
                let mut audio_frequency__ = None;
                let mut video_codec__ = None;
                let mut video_bitrate__ = None;
                let mut video_quality__ = None;
                let mut key_frame_interval__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Width => {
                            if width__.is_some() {
                                return Err(serde::de::Error::duplicate_field("width"));
                            }
                            width__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Depth => {
                            if depth__.is_some() {
                                return Err(serde::de::Error::duplicate_field("depth"));
                            }
                            depth__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Framerate => {
                            if framerate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("framerate"));
                            }
                            framerate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AudioCodec => {
                            if audio_codec__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioCodec"));
                            }
                            audio_codec__ = Some(map.next_value::<AudioCodec>()? as i32);
                        }
                        GeneratedField::AudioBitrate => {
                            if audio_bitrate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioBitrate"));
                            }
                            audio_bitrate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AudioQuality => {
                            if audio_quality__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioQuality"));
                            }
                            audio_quality__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AudioFrequency => {
                            if audio_frequency__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioFrequency"));
                            }
                            audio_frequency__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::VideoCodec => {
                            if video_codec__.is_some() {
                                return Err(serde::de::Error::duplicate_field("videoCodec"));
                            }
                            video_codec__ = Some(map.next_value::<VideoCodec>()? as i32);
                        }
                        GeneratedField::VideoBitrate => {
                            if video_bitrate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("videoBitrate"));
                            }
                            video_bitrate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::VideoQuality => {
                            if video_quality__.is_some() {
                                return Err(serde::de::Error::duplicate_field("videoQuality"));
                            }
                            video_quality__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::KeyFrameInterval => {
                            if key_frame_interval__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyFrameInterval"));
                            }
                            key_frame_interval__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(EncodingOptions {
                    width: width__.unwrap_or_default(),
                    height: height__.unwrap_or_default(),
                    depth: depth__.unwrap_or_default(),
                    framerate: framerate__.unwrap_or_default(),
                    audio_codec: audio_codec__.unwrap_or_default(),
                    audio_bitrate: audio_bitrate__.unwrap_or_default(),
                    audio_quality: audio_quality__.unwrap_or_default(),
                    audio_frequency: audio_frequency__.unwrap_or_default(),
                    video_codec: video_codec__.unwrap_or_default(),
                    video_bitrate: video_bitrate__.unwrap_or_default(),
                    video_quality: video_quality__.unwrap_or_default(),
                    key_frame_interval: key_frame_interval__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.EncodingOptions", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EncodingOptionsPreset {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::H264720p30 => "H264_720P_30",
            Self::H264720p60 => "H264_720P_60",
            Self::H2641080p30 => "H264_1080P_30",
            Self::H2641080p60 => "H264_1080P_60",
            Self::PortraitH264720p30 => "PORTRAIT_H264_720P_30",
            Self::PortraitH264720p60 => "PORTRAIT_H264_720P_60",
            Self::PortraitH2641080p30 => "PORTRAIT_H264_1080P_30",
            Self::PortraitH2641080p60 => "PORTRAIT_H264_1080P_60",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for EncodingOptionsPreset {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "H264_720P_30",
            "H264_720P_60",
            "H264_1080P_30",
            "H264_1080P_60",
            "PORTRAIT_H264_720P_30",
            "PORTRAIT_H264_720P_60",
            "PORTRAIT_H264_1080P_30",
            "PORTRAIT_H264_1080P_60",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EncodingOptionsPreset;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(EncodingOptionsPreset::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(EncodingOptionsPreset::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "H264_720P_30" => Ok(EncodingOptionsPreset::H264720p30),
                    "H264_720P_60" => Ok(EncodingOptionsPreset::H264720p60),
                    "H264_1080P_30" => Ok(EncodingOptionsPreset::H2641080p30),
                    "H264_1080P_60" => Ok(EncodingOptionsPreset::H2641080p60),
                    "PORTRAIT_H264_720P_30" => Ok(EncodingOptionsPreset::PortraitH264720p30),
                    "PORTRAIT_H264_720P_60" => Ok(EncodingOptionsPreset::PortraitH264720p60),
                    "PORTRAIT_H264_1080P_30" => Ok(EncodingOptionsPreset::PortraitH2641080p30),
                    "PORTRAIT_H264_1080P_60" => Ok(EncodingOptionsPreset::PortraitH2641080p60),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for Encryption {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.Encryption", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Encryption {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Err(serde::de::Error::unknown_field(value, FIELDS))
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Encryption;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.Encryption")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<Encryption, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map.next_key::<GeneratedField>()?.is_some() {
                    let _ = map.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(Encryption {
                })
            }
        }
        deserializer.deserialize_struct("livekit.Encryption", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for encryption::Type {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::None => "NONE",
            Self::Gcm => "GCM",
            Self::Custom => "CUSTOM",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for encryption::Type {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "NONE",
            "GCM",
            "CUSTOM",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = encryption::Type;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(encryption::Type::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(encryption::Type::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "NONE" => Ok(encryption::Type::None),
                    "GCM" => Ok(encryption::Type::Gcm),
                    "CUSTOM" => Ok(encryption::Type::Custom),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for FileInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.filename.is_empty() {
            len += 1;
        }
        if self.started_at != 0 {
            len += 1;
        }
        if self.ended_at != 0 {
            len += 1;
        }
        if self.duration != 0 {
            len += 1;
        }
        if self.size != 0 {
            len += 1;
        }
        if !self.location.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.FileInfo", len)?;
        if !self.filename.is_empty() {
            struct_ser.serialize_field("filename", &self.filename)?;
        }
        if self.started_at != 0 {
            struct_ser.serialize_field("startedAt", ToString::to_string(&self.started_at).as_str())?;
        }
        if self.ended_at != 0 {
            struct_ser.serialize_field("endedAt", ToString::to_string(&self.ended_at).as_str())?;
        }
        if self.duration != 0 {
            struct_ser.serialize_field("duration", ToString::to_string(&self.duration).as_str())?;
        }
        if self.size != 0 {
            struct_ser.serialize_field("size", ToString::to_string(&self.size).as_str())?;
        }
        if !self.location.is_empty() {
            struct_ser.serialize_field("location", &self.location)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FileInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "filename",
            "started_at",
            "startedAt",
            "ended_at",
            "endedAt",
            "duration",
            "size",
            "location",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Filename,
            StartedAt,
            EndedAt,
            Duration,
            Size,
            Location,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "filename" => Ok(GeneratedField::Filename),
                            "startedAt" | "started_at" => Ok(GeneratedField::StartedAt),
                            "endedAt" | "ended_at" => Ok(GeneratedField::EndedAt),
                            "duration" => Ok(GeneratedField::Duration),
                            "size" => Ok(GeneratedField::Size),
                            "location" => Ok(GeneratedField::Location),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FileInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.FileInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<FileInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut filename__ = None;
                let mut started_at__ = None;
                let mut ended_at__ = None;
                let mut duration__ = None;
                let mut size__ = None;
                let mut location__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Filename => {
                            if filename__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filename"));
                            }
                            filename__ = Some(map.next_value()?);
                        }
                        GeneratedField::StartedAt => {
                            if started_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startedAt"));
                            }
                            started_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndedAt => {
                            if ended_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endedAt"));
                            }
                            ended_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Duration => {
                            if duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("duration"));
                            }
                            duration__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Size => {
                            if size__.is_some() {
                                return Err(serde::de::Error::duplicate_field("size"));
                            }
                            size__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Location => {
                            if location__.is_some() {
                                return Err(serde::de::Error::duplicate_field("location"));
                            }
                            location__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(FileInfo {
                    filename: filename__.unwrap_or_default(),
                    started_at: started_at__.unwrap_or_default(),
                    ended_at: ended_at__.unwrap_or_default(),
                    duration: duration__.unwrap_or_default(),
                    size: size__.unwrap_or_default(),
                    location: location__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.FileInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GcpUpload {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.credentials.is_empty() {
            len += 1;
        }
        if !self.bucket.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.GCPUpload", len)?;
        if !self.credentials.is_empty() {
            struct_ser.serialize_field("credentials", &self.credentials)?;
        }
        if !self.bucket.is_empty() {
            struct_ser.serialize_field("bucket", &self.bucket)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GcpUpload {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "credentials",
            "bucket",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Credentials,
            Bucket,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "credentials" => Ok(GeneratedField::Credentials),
                            "bucket" => Ok(GeneratedField::Bucket),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GcpUpload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.GCPUpload")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<GcpUpload, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut credentials__ = None;
                let mut bucket__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Credentials => {
                            if credentials__.is_some() {
                                return Err(serde::de::Error::duplicate_field("credentials"));
                            }
                            credentials__ = Some(map.next_value()?);
                        }
                        GeneratedField::Bucket => {
                            if bucket__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bucket"));
                            }
                            bucket__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(GcpUpload {
                    credentials: credentials__.unwrap_or_default(),
                    bucket: bucket__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.GCPUpload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IceServer {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.urls.is_empty() {
            len += 1;
        }
        if !self.username.is_empty() {
            len += 1;
        }
        if !self.credential.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ICEServer", len)?;
        if !self.urls.is_empty() {
            struct_ser.serialize_field("urls", &self.urls)?;
        }
        if !self.username.is_empty() {
            struct_ser.serialize_field("username", &self.username)?;
        }
        if !self.credential.is_empty() {
            struct_ser.serialize_field("credential", &self.credential)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IceServer {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "urls",
            "username",
            "credential",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Urls,
            Username,
            Credential,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "urls" => Ok(GeneratedField::Urls),
                            "username" => Ok(GeneratedField::Username),
                            "credential" => Ok(GeneratedField::Credential),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IceServer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ICEServer")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<IceServer, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut urls__ = None;
                let mut username__ = None;
                let mut credential__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Urls => {
                            if urls__.is_some() {
                                return Err(serde::de::Error::duplicate_field("urls"));
                            }
                            urls__ = Some(map.next_value()?);
                        }
                        GeneratedField::Username => {
                            if username__.is_some() {
                                return Err(serde::de::Error::duplicate_field("username"));
                            }
                            username__ = Some(map.next_value()?);
                        }
                        GeneratedField::Credential => {
                            if credential__.is_some() {
                                return Err(serde::de::Error::duplicate_field("credential"));
                            }
                            credential__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(IceServer {
                    urls: urls__.unwrap_or_default(),
                    username: username__.unwrap_or_default(),
                    credential: credential__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ICEServer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ImageCodec {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::IcDefault => "IC_DEFAULT",
            Self::IcJpeg => "IC_JPEG",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ImageCodec {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "IC_DEFAULT",
            "IC_JPEG",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ImageCodec;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ImageCodec::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ImageCodec::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "IC_DEFAULT" => Ok(ImageCodec::IcDefault),
                    "IC_JPEG" => Ok(ImageCodec::IcJpeg),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ImageFileSuffix {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::ImageSuffixIndex => "IMAGE_SUFFIX_INDEX",
            Self::ImageSuffixTimestamp => "IMAGE_SUFFIX_TIMESTAMP",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ImageFileSuffix {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "IMAGE_SUFFIX_INDEX",
            "IMAGE_SUFFIX_TIMESTAMP",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ImageFileSuffix;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ImageFileSuffix::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ImageFileSuffix::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "IMAGE_SUFFIX_INDEX" => Ok(ImageFileSuffix::ImageSuffixIndex),
                    "IMAGE_SUFFIX_TIMESTAMP" => Ok(ImageFileSuffix::ImageSuffixTimestamp),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ImageOutput {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.capture_interval != 0 {
            len += 1;
        }
        if self.width != 0 {
            len += 1;
        }
        if self.height != 0 {
            len += 1;
        }
        if !self.filename_prefix.is_empty() {
            len += 1;
        }
        if self.filename_suffix != 0 {
            len += 1;
        }
        if self.image_codec != 0 {
            len += 1;
        }
        if self.disable_manifest {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ImageOutput", len)?;
        if self.capture_interval != 0 {
            struct_ser.serialize_field("captureInterval", &self.capture_interval)?;
        }
        if self.width != 0 {
            struct_ser.serialize_field("width", &self.width)?;
        }
        if self.height != 0 {
            struct_ser.serialize_field("height", &self.height)?;
        }
        if !self.filename_prefix.is_empty() {
            struct_ser.serialize_field("filenamePrefix", &self.filename_prefix)?;
        }
        if self.filename_suffix != 0 {
            let v = ImageFileSuffix::from_i32(self.filename_suffix)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.filename_suffix)))?;
            struct_ser.serialize_field("filenameSuffix", &v)?;
        }
        if self.image_codec != 0 {
            let v = ImageCodec::from_i32(self.image_codec)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.image_codec)))?;
            struct_ser.serialize_field("imageCodec", &v)?;
        }
        if self.disable_manifest {
            struct_ser.serialize_field("disableManifest", &self.disable_manifest)?;
        }
        if let Some(v) = self.output.as_ref() {
            match v {
                image_output::Output::S3(v) => {
                    struct_ser.serialize_field("s3", v)?;
                }
                image_output::Output::Gcp(v) => {
                    struct_ser.serialize_field("gcp", v)?;
                }
                image_output::Output::Azure(v) => {
                    struct_ser.serialize_field("azure", v)?;
                }
                image_output::Output::AliOss(v) => {
                    struct_ser.serialize_field("aliOSS", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ImageOutput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "capture_interval",
            "captureInterval",
            "width",
            "height",
            "filename_prefix",
            "filenamePrefix",
            "filename_suffix",
            "filenameSuffix",
            "image_codec",
            "imageCodec",
            "disable_manifest",
            "disableManifest",
            "s3",
            "gcp",
            "azure",
            "aliOSS",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CaptureInterval,
            Width,
            Height,
            FilenamePrefix,
            FilenameSuffix,
            ImageCodec,
            DisableManifest,
            S3,
            Gcp,
            Azure,
            AliOss,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "captureInterval" | "capture_interval" => Ok(GeneratedField::CaptureInterval),
                            "width" => Ok(GeneratedField::Width),
                            "height" => Ok(GeneratedField::Height),
                            "filenamePrefix" | "filename_prefix" => Ok(GeneratedField::FilenamePrefix),
                            "filenameSuffix" | "filename_suffix" => Ok(GeneratedField::FilenameSuffix),
                            "imageCodec" | "image_codec" => Ok(GeneratedField::ImageCodec),
                            "disableManifest" | "disable_manifest" => Ok(GeneratedField::DisableManifest),
                            "s3" => Ok(GeneratedField::S3),
                            "gcp" => Ok(GeneratedField::Gcp),
                            "azure" => Ok(GeneratedField::Azure),
                            "aliOSS" => Ok(GeneratedField::AliOss),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ImageOutput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ImageOutput")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ImageOutput, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut capture_interval__ = None;
                let mut width__ = None;
                let mut height__ = None;
                let mut filename_prefix__ = None;
                let mut filename_suffix__ = None;
                let mut image_codec__ = None;
                let mut disable_manifest__ = None;
                let mut output__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::CaptureInterval => {
                            if capture_interval__.is_some() {
                                return Err(serde::de::Error::duplicate_field("captureInterval"));
                            }
                            capture_interval__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Width => {
                            if width__.is_some() {
                                return Err(serde::de::Error::duplicate_field("width"));
                            }
                            width__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::FilenamePrefix => {
                            if filename_prefix__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filenamePrefix"));
                            }
                            filename_prefix__ = Some(map.next_value()?);
                        }
                        GeneratedField::FilenameSuffix => {
                            if filename_suffix__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filenameSuffix"));
                            }
                            filename_suffix__ = Some(map.next_value::<ImageFileSuffix>()? as i32);
                        }
                        GeneratedField::ImageCodec => {
                            if image_codec__.is_some() {
                                return Err(serde::de::Error::duplicate_field("imageCodec"));
                            }
                            image_codec__ = Some(map.next_value::<ImageCodec>()? as i32);
                        }
                        GeneratedField::DisableManifest => {
                            if disable_manifest__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableManifest"));
                            }
                            disable_manifest__ = Some(map.next_value()?);
                        }
                        GeneratedField::S3 => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("s3"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(image_output::Output::S3)
;
                        }
                        GeneratedField::Gcp => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("gcp"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(image_output::Output::Gcp)
;
                        }
                        GeneratedField::Azure => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("azure"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(image_output::Output::Azure)
;
                        }
                        GeneratedField::AliOss => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("aliOSS"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(image_output::Output::AliOss)
;
                        }
                    }
                }
                Ok(ImageOutput {
                    capture_interval: capture_interval__.unwrap_or_default(),
                    width: width__.unwrap_or_default(),
                    height: height__.unwrap_or_default(),
                    filename_prefix: filename_prefix__.unwrap_or_default(),
                    filename_suffix: filename_suffix__.unwrap_or_default(),
                    image_codec: image_codec__.unwrap_or_default(),
                    disable_manifest: disable_manifest__.unwrap_or_default(),
                    output: output__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.ImageOutput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ImagesInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.image_count != 0 {
            len += 1;
        }
        if self.started_at != 0 {
            len += 1;
        }
        if self.ended_at != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ImagesInfo", len)?;
        if self.image_count != 0 {
            struct_ser.serialize_field("imageCount", ToString::to_string(&self.image_count).as_str())?;
        }
        if self.started_at != 0 {
            struct_ser.serialize_field("startedAt", ToString::to_string(&self.started_at).as_str())?;
        }
        if self.ended_at != 0 {
            struct_ser.serialize_field("endedAt", ToString::to_string(&self.ended_at).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ImagesInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "image_count",
            "imageCount",
            "started_at",
            "startedAt",
            "ended_at",
            "endedAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ImageCount,
            StartedAt,
            EndedAt,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "imageCount" | "image_count" => Ok(GeneratedField::ImageCount),
                            "startedAt" | "started_at" => Ok(GeneratedField::StartedAt),
                            "endedAt" | "ended_at" => Ok(GeneratedField::EndedAt),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ImagesInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ImagesInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ImagesInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut image_count__ = None;
                let mut started_at__ = None;
                let mut ended_at__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::ImageCount => {
                            if image_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("imageCount"));
                            }
                            image_count__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::StartedAt => {
                            if started_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startedAt"));
                            }
                            started_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndedAt => {
                            if ended_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endedAt"));
                            }
                            ended_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(ImagesInfo {
                    image_count: image_count__.unwrap_or_default(),
                    started_at: started_at__.unwrap_or_default(),
                    ended_at: ended_at__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ImagesInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IngressAudioEncodingOptions {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.audio_codec != 0 {
            len += 1;
        }
        if self.bitrate != 0 {
            len += 1;
        }
        if self.disable_dtx {
            len += 1;
        }
        if self.channels != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.IngressAudioEncodingOptions", len)?;
        if self.audio_codec != 0 {
            let v = AudioCodec::from_i32(self.audio_codec)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.audio_codec)))?;
            struct_ser.serialize_field("audioCodec", &v)?;
        }
        if self.bitrate != 0 {
            struct_ser.serialize_field("bitrate", &self.bitrate)?;
        }
        if self.disable_dtx {
            struct_ser.serialize_field("disableDtx", &self.disable_dtx)?;
        }
        if self.channels != 0 {
            struct_ser.serialize_field("channels", &self.channels)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IngressAudioEncodingOptions {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "audio_codec",
            "audioCodec",
            "bitrate",
            "disable_dtx",
            "disableDtx",
            "channels",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AudioCodec,
            Bitrate,
            DisableDtx,
            Channels,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "audioCodec" | "audio_codec" => Ok(GeneratedField::AudioCodec),
                            "bitrate" => Ok(GeneratedField::Bitrate),
                            "disableDtx" | "disable_dtx" => Ok(GeneratedField::DisableDtx),
                            "channels" => Ok(GeneratedField::Channels),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IngressAudioEncodingOptions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.IngressAudioEncodingOptions")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<IngressAudioEncodingOptions, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut audio_codec__ = None;
                let mut bitrate__ = None;
                let mut disable_dtx__ = None;
                let mut channels__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::AudioCodec => {
                            if audio_codec__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioCodec"));
                            }
                            audio_codec__ = Some(map.next_value::<AudioCodec>()? as i32);
                        }
                        GeneratedField::Bitrate => {
                            if bitrate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bitrate"));
                            }
                            bitrate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DisableDtx => {
                            if disable_dtx__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableDtx"));
                            }
                            disable_dtx__ = Some(map.next_value()?);
                        }
                        GeneratedField::Channels => {
                            if channels__.is_some() {
                                return Err(serde::de::Error::duplicate_field("channels"));
                            }
                            channels__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(IngressAudioEncodingOptions {
                    audio_codec: audio_codec__.unwrap_or_default(),
                    bitrate: bitrate__.unwrap_or_default(),
                    disable_dtx: disable_dtx__.unwrap_or_default(),
                    channels: channels__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.IngressAudioEncodingOptions", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IngressAudioEncodingPreset {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::OpusStereo96kbps => "OPUS_STEREO_96KBPS",
            Self::OpusMono64kbs => "OPUS_MONO_64KBS",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for IngressAudioEncodingPreset {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "OPUS_STEREO_96KBPS",
            "OPUS_MONO_64KBS",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IngressAudioEncodingPreset;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(IngressAudioEncodingPreset::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(IngressAudioEncodingPreset::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "OPUS_STEREO_96KBPS" => Ok(IngressAudioEncodingPreset::OpusStereo96kbps),
                    "OPUS_MONO_64KBS" => Ok(IngressAudioEncodingPreset::OpusMono64kbs),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for IngressAudioOptions {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.name.is_empty() {
            len += 1;
        }
        if self.source != 0 {
            len += 1;
        }
        if self.encoding_options.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.IngressAudioOptions", len)?;
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if self.source != 0 {
            let v = TrackSource::from_i32(self.source)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.source)))?;
            struct_ser.serialize_field("source", &v)?;
        }
        if let Some(v) = self.encoding_options.as_ref() {
            match v {
                ingress_audio_options::EncodingOptions::Preset(v) => {
                    let v = IngressAudioEncodingPreset::from_i32(*v)
                        .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("preset", &v)?;
                }
                ingress_audio_options::EncodingOptions::Options(v) => {
                    struct_ser.serialize_field("options", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IngressAudioOptions {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "name",
            "source",
            "preset",
            "options",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Name,
            Source,
            Preset,
            Options,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "name" => Ok(GeneratedField::Name),
                            "source" => Ok(GeneratedField::Source),
                            "preset" => Ok(GeneratedField::Preset),
                            "options" => Ok(GeneratedField::Options),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IngressAudioOptions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.IngressAudioOptions")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<IngressAudioOptions, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut name__ = None;
                let mut source__ = None;
                let mut encoding_options__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Source => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source__ = Some(map.next_value::<TrackSource>()? as i32);
                        }
                        GeneratedField::Preset => {
                            if encoding_options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preset"));
                            }
                            encoding_options__ = map.next_value::<::std::option::Option<IngressAudioEncodingPreset>>()?.map(|x| ingress_audio_options::EncodingOptions::Preset(x as i32));
                        }
                        GeneratedField::Options => {
                            if encoding_options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("options"));
                            }
                            encoding_options__ = map.next_value::<::std::option::Option<_>>()?.map(ingress_audio_options::EncodingOptions::Options)
;
                        }
                    }
                }
                Ok(IngressAudioOptions {
                    name: name__.unwrap_or_default(),
                    source: source__.unwrap_or_default(),
                    encoding_options: encoding_options__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.IngressAudioOptions", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IngressInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.ingress_id.is_empty() {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        if !self.stream_key.is_empty() {
            len += 1;
        }
        if !self.url.is_empty() {
            len += 1;
        }
        if self.input_type != 0 {
            len += 1;
        }
        if self.bypass_transcoding {
            len += 1;
        }
        if self.audio.is_some() {
            len += 1;
        }
        if self.video.is_some() {
            len += 1;
        }
        if !self.room_name.is_empty() {
            len += 1;
        }
        if !self.participant_identity.is_empty() {
            len += 1;
        }
        if !self.participant_name.is_empty() {
            len += 1;
        }
        if self.reusable {
            len += 1;
        }
        if self.state.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.IngressInfo", len)?;
        if !self.ingress_id.is_empty() {
            struct_ser.serialize_field("ingressId", &self.ingress_id)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if !self.stream_key.is_empty() {
            struct_ser.serialize_field("streamKey", &self.stream_key)?;
        }
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        if self.input_type != 0 {
            let v = IngressInput::from_i32(self.input_type)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.input_type)))?;
            struct_ser.serialize_field("inputType", &v)?;
        }
        if self.bypass_transcoding {
            struct_ser.serialize_field("bypassTranscoding", &self.bypass_transcoding)?;
        }
        if let Some(v) = self.audio.as_ref() {
            struct_ser.serialize_field("audio", v)?;
        }
        if let Some(v) = self.video.as_ref() {
            struct_ser.serialize_field("video", v)?;
        }
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if !self.participant_identity.is_empty() {
            struct_ser.serialize_field("participantIdentity", &self.participant_identity)?;
        }
        if !self.participant_name.is_empty() {
            struct_ser.serialize_field("participantName", &self.participant_name)?;
        }
        if self.reusable {
            struct_ser.serialize_field("reusable", &self.reusable)?;
        }
        if let Some(v) = self.state.as_ref() {
            struct_ser.serialize_field("state", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IngressInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ingress_id",
            "ingressId",
            "name",
            "stream_key",
            "streamKey",
            "url",
            "input_type",
            "inputType",
            "bypass_transcoding",
            "bypassTranscoding",
            "audio",
            "video",
            "room_name",
            "roomName",
            "participant_identity",
            "participantIdentity",
            "participant_name",
            "participantName",
            "reusable",
            "state",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IngressId,
            Name,
            StreamKey,
            Url,
            InputType,
            BypassTranscoding,
            Audio,
            Video,
            RoomName,
            ParticipantIdentity,
            ParticipantName,
            Reusable,
            State,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "ingressId" | "ingress_id" => Ok(GeneratedField::IngressId),
                            "name" => Ok(GeneratedField::Name),
                            "streamKey" | "stream_key" => Ok(GeneratedField::StreamKey),
                            "url" => Ok(GeneratedField::Url),
                            "inputType" | "input_type" => Ok(GeneratedField::InputType),
                            "bypassTranscoding" | "bypass_transcoding" => Ok(GeneratedField::BypassTranscoding),
                            "audio" => Ok(GeneratedField::Audio),
                            "video" => Ok(GeneratedField::Video),
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "participantIdentity" | "participant_identity" => Ok(GeneratedField::ParticipantIdentity),
                            "participantName" | "participant_name" => Ok(GeneratedField::ParticipantName),
                            "reusable" => Ok(GeneratedField::Reusable),
                            "state" => Ok(GeneratedField::State),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IngressInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.IngressInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<IngressInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut ingress_id__ = None;
                let mut name__ = None;
                let mut stream_key__ = None;
                let mut url__ = None;
                let mut input_type__ = None;
                let mut bypass_transcoding__ = None;
                let mut audio__ = None;
                let mut video__ = None;
                let mut room_name__ = None;
                let mut participant_identity__ = None;
                let mut participant_name__ = None;
                let mut reusable__ = None;
                let mut state__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IngressId => {
                            if ingress_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ingressId"));
                            }
                            ingress_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::StreamKey => {
                            if stream_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("streamKey"));
                            }
                            stream_key__ = Some(map.next_value()?);
                        }
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map.next_value()?);
                        }
                        GeneratedField::InputType => {
                            if input_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputType"));
                            }
                            input_type__ = Some(map.next_value::<IngressInput>()? as i32);
                        }
                        GeneratedField::BypassTranscoding => {
                            if bypass_transcoding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bypassTranscoding"));
                            }
                            bypass_transcoding__ = Some(map.next_value()?);
                        }
                        GeneratedField::Audio => {
                            if audio__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audio"));
                            }
                            audio__ = map.next_value()?;
                        }
                        GeneratedField::Video => {
                            if video__.is_some() {
                                return Err(serde::de::Error::duplicate_field("video"));
                            }
                            video__ = map.next_value()?;
                        }
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantIdentity => {
                            if participant_identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantIdentity"));
                            }
                            participant_identity__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantName => {
                            if participant_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantName"));
                            }
                            participant_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Reusable => {
                            if reusable__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reusable"));
                            }
                            reusable__ = Some(map.next_value()?);
                        }
                        GeneratedField::State => {
                            if state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("state"));
                            }
                            state__ = map.next_value()?;
                        }
                    }
                }
                Ok(IngressInfo {
                    ingress_id: ingress_id__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                    stream_key: stream_key__.unwrap_or_default(),
                    url: url__.unwrap_or_default(),
                    input_type: input_type__.unwrap_or_default(),
                    bypass_transcoding: bypass_transcoding__.unwrap_or_default(),
                    audio: audio__,
                    video: video__,
                    room_name: room_name__.unwrap_or_default(),
                    participant_identity: participant_identity__.unwrap_or_default(),
                    participant_name: participant_name__.unwrap_or_default(),
                    reusable: reusable__.unwrap_or_default(),
                    state: state__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.IngressInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IngressInput {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::RtmpInput => "RTMP_INPUT",
            Self::WhipInput => "WHIP_INPUT",
            Self::UrlInput => "URL_INPUT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for IngressInput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RTMP_INPUT",
            "WHIP_INPUT",
            "URL_INPUT",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IngressInput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(IngressInput::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(IngressInput::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RTMP_INPUT" => Ok(IngressInput::RtmpInput),
                    "WHIP_INPUT" => Ok(IngressInput::WhipInput),
                    "URL_INPUT" => Ok(IngressInput::UrlInput),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for IngressState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.status != 0 {
            len += 1;
        }
        if !self.error.is_empty() {
            len += 1;
        }
        if self.video.is_some() {
            len += 1;
        }
        if self.audio.is_some() {
            len += 1;
        }
        if !self.room_id.is_empty() {
            len += 1;
        }
        if self.started_at != 0 {
            len += 1;
        }
        if self.ended_at != 0 {
            len += 1;
        }
        if !self.resource_id.is_empty() {
            len += 1;
        }
        if !self.tracks.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.IngressState", len)?;
        if self.status != 0 {
            let v = ingress_state::Status::from_i32(self.status)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.status)))?;
            struct_ser.serialize_field("status", &v)?;
        }
        if !self.error.is_empty() {
            struct_ser.serialize_field("error", &self.error)?;
        }
        if let Some(v) = self.video.as_ref() {
            struct_ser.serialize_field("video", v)?;
        }
        if let Some(v) = self.audio.as_ref() {
            struct_ser.serialize_field("audio", v)?;
        }
        if !self.room_id.is_empty() {
            struct_ser.serialize_field("roomId", &self.room_id)?;
        }
        if self.started_at != 0 {
            struct_ser.serialize_field("startedAt", ToString::to_string(&self.started_at).as_str())?;
        }
        if self.ended_at != 0 {
            struct_ser.serialize_field("endedAt", ToString::to_string(&self.ended_at).as_str())?;
        }
        if !self.resource_id.is_empty() {
            struct_ser.serialize_field("resourceId", &self.resource_id)?;
        }
        if !self.tracks.is_empty() {
            struct_ser.serialize_field("tracks", &self.tracks)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IngressState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "status",
            "error",
            "video",
            "audio",
            "room_id",
            "roomId",
            "started_at",
            "startedAt",
            "ended_at",
            "endedAt",
            "resource_id",
            "resourceId",
            "tracks",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Status,
            Error,
            Video,
            Audio,
            RoomId,
            StartedAt,
            EndedAt,
            ResourceId,
            Tracks,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "status" => Ok(GeneratedField::Status),
                            "error" => Ok(GeneratedField::Error),
                            "video" => Ok(GeneratedField::Video),
                            "audio" => Ok(GeneratedField::Audio),
                            "roomId" | "room_id" => Ok(GeneratedField::RoomId),
                            "startedAt" | "started_at" => Ok(GeneratedField::StartedAt),
                            "endedAt" | "ended_at" => Ok(GeneratedField::EndedAt),
                            "resourceId" | "resource_id" => Ok(GeneratedField::ResourceId),
                            "tracks" => Ok(GeneratedField::Tracks),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IngressState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.IngressState")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<IngressState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut status__ = None;
                let mut error__ = None;
                let mut video__ = None;
                let mut audio__ = None;
                let mut room_id__ = None;
                let mut started_at__ = None;
                let mut ended_at__ = None;
                let mut resource_id__ = None;
                let mut tracks__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Status => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("status"));
                            }
                            status__ = Some(map.next_value::<ingress_state::Status>()? as i32);
                        }
                        GeneratedField::Error => {
                            if error__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            error__ = Some(map.next_value()?);
                        }
                        GeneratedField::Video => {
                            if video__.is_some() {
                                return Err(serde::de::Error::duplicate_field("video"));
                            }
                            video__ = map.next_value()?;
                        }
                        GeneratedField::Audio => {
                            if audio__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audio"));
                            }
                            audio__ = map.next_value()?;
                        }
                        GeneratedField::RoomId => {
                            if room_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomId"));
                            }
                            room_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::StartedAt => {
                            if started_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startedAt"));
                            }
                            started_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndedAt => {
                            if ended_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endedAt"));
                            }
                            ended_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ResourceId => {
                            if resource_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("resourceId"));
                            }
                            resource_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::Tracks => {
                            if tracks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tracks"));
                            }
                            tracks__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(IngressState {
                    status: status__.unwrap_or_default(),
                    error: error__.unwrap_or_default(),
                    video: video__,
                    audio: audio__,
                    room_id: room_id__.unwrap_or_default(),
                    started_at: started_at__.unwrap_or_default(),
                    ended_at: ended_at__.unwrap_or_default(),
                    resource_id: resource_id__.unwrap_or_default(),
                    tracks: tracks__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.IngressState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ingress_state::Status {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::EndpointInactive => "ENDPOINT_INACTIVE",
            Self::EndpointBuffering => "ENDPOINT_BUFFERING",
            Self::EndpointPublishing => "ENDPOINT_PUBLISHING",
            Self::EndpointError => "ENDPOINT_ERROR",
            Self::EndpointComplete => "ENDPOINT_COMPLETE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ingress_state::Status {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ENDPOINT_INACTIVE",
            "ENDPOINT_BUFFERING",
            "ENDPOINT_PUBLISHING",
            "ENDPOINT_ERROR",
            "ENDPOINT_COMPLETE",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ingress_state::Status;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ingress_state::Status::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ingress_state::Status::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ENDPOINT_INACTIVE" => Ok(ingress_state::Status::EndpointInactive),
                    "ENDPOINT_BUFFERING" => Ok(ingress_state::Status::EndpointBuffering),
                    "ENDPOINT_PUBLISHING" => Ok(ingress_state::Status::EndpointPublishing),
                    "ENDPOINT_ERROR" => Ok(ingress_state::Status::EndpointError),
                    "ENDPOINT_COMPLETE" => Ok(ingress_state::Status::EndpointComplete),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for IngressVideoEncodingOptions {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.video_codec != 0 {
            len += 1;
        }
        if self.frame_rate != 0. {
            len += 1;
        }
        if !self.layers.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.IngressVideoEncodingOptions", len)?;
        if self.video_codec != 0 {
            let v = VideoCodec::from_i32(self.video_codec)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.video_codec)))?;
            struct_ser.serialize_field("videoCodec", &v)?;
        }
        if self.frame_rate != 0. {
            struct_ser.serialize_field("frameRate", &self.frame_rate)?;
        }
        if !self.layers.is_empty() {
            struct_ser.serialize_field("layers", &self.layers)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IngressVideoEncodingOptions {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "video_codec",
            "videoCodec",
            "frame_rate",
            "frameRate",
            "layers",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            VideoCodec,
            FrameRate,
            Layers,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "videoCodec" | "video_codec" => Ok(GeneratedField::VideoCodec),
                            "frameRate" | "frame_rate" => Ok(GeneratedField::FrameRate),
                            "layers" => Ok(GeneratedField::Layers),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IngressVideoEncodingOptions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.IngressVideoEncodingOptions")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<IngressVideoEncodingOptions, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut video_codec__ = None;
                let mut frame_rate__ = None;
                let mut layers__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::VideoCodec => {
                            if video_codec__.is_some() {
                                return Err(serde::de::Error::duplicate_field("videoCodec"));
                            }
                            video_codec__ = Some(map.next_value::<VideoCodec>()? as i32);
                        }
                        GeneratedField::FrameRate => {
                            if frame_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("frameRate"));
                            }
                            frame_rate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Layers => {
                            if layers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("layers"));
                            }
                            layers__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(IngressVideoEncodingOptions {
                    video_codec: video_codec__.unwrap_or_default(),
                    frame_rate: frame_rate__.unwrap_or_default(),
                    layers: layers__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.IngressVideoEncodingOptions", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IngressVideoEncodingPreset {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::H264720p30fps3Layers => "H264_720P_30FPS_3_LAYERS",
            Self::H2641080p30fps3Layers => "H264_1080P_30FPS_3_LAYERS",
            Self::H264540p25fps2Layers => "H264_540P_25FPS_2_LAYERS",
            Self::H264720p30fps1Layer => "H264_720P_30FPS_1_LAYER",
            Self::H2641080p30fps1Layer => "H264_1080P_30FPS_1_LAYER",
            Self::H264720p30fps3LayersHighMotion => "H264_720P_30FPS_3_LAYERS_HIGH_MOTION",
            Self::H2641080p30fps3LayersHighMotion => "H264_1080P_30FPS_3_LAYERS_HIGH_MOTION",
            Self::H264540p25fps2LayersHighMotion => "H264_540P_25FPS_2_LAYERS_HIGH_MOTION",
            Self::H264720p30fps1LayerHighMotion => "H264_720P_30FPS_1_LAYER_HIGH_MOTION",
            Self::H2641080p30fps1LayerHighMotion => "H264_1080P_30FPS_1_LAYER_HIGH_MOTION",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for IngressVideoEncodingPreset {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "H264_720P_30FPS_3_LAYERS",
            "H264_1080P_30FPS_3_LAYERS",
            "H264_540P_25FPS_2_LAYERS",
            "H264_720P_30FPS_1_LAYER",
            "H264_1080P_30FPS_1_LAYER",
            "H264_720P_30FPS_3_LAYERS_HIGH_MOTION",
            "H264_1080P_30FPS_3_LAYERS_HIGH_MOTION",
            "H264_540P_25FPS_2_LAYERS_HIGH_MOTION",
            "H264_720P_30FPS_1_LAYER_HIGH_MOTION",
            "H264_1080P_30FPS_1_LAYER_HIGH_MOTION",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IngressVideoEncodingPreset;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(IngressVideoEncodingPreset::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(IngressVideoEncodingPreset::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "H264_720P_30FPS_3_LAYERS" => Ok(IngressVideoEncodingPreset::H264720p30fps3Layers),
                    "H264_1080P_30FPS_3_LAYERS" => Ok(IngressVideoEncodingPreset::H2641080p30fps3Layers),
                    "H264_540P_25FPS_2_LAYERS" => Ok(IngressVideoEncodingPreset::H264540p25fps2Layers),
                    "H264_720P_30FPS_1_LAYER" => Ok(IngressVideoEncodingPreset::H264720p30fps1Layer),
                    "H264_1080P_30FPS_1_LAYER" => Ok(IngressVideoEncodingPreset::H2641080p30fps1Layer),
                    "H264_720P_30FPS_3_LAYERS_HIGH_MOTION" => Ok(IngressVideoEncodingPreset::H264720p30fps3LayersHighMotion),
                    "H264_1080P_30FPS_3_LAYERS_HIGH_MOTION" => Ok(IngressVideoEncodingPreset::H2641080p30fps3LayersHighMotion),
                    "H264_540P_25FPS_2_LAYERS_HIGH_MOTION" => Ok(IngressVideoEncodingPreset::H264540p25fps2LayersHighMotion),
                    "H264_720P_30FPS_1_LAYER_HIGH_MOTION" => Ok(IngressVideoEncodingPreset::H264720p30fps1LayerHighMotion),
                    "H264_1080P_30FPS_1_LAYER_HIGH_MOTION" => Ok(IngressVideoEncodingPreset::H2641080p30fps1LayerHighMotion),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for IngressVideoOptions {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.name.is_empty() {
            len += 1;
        }
        if self.source != 0 {
            len += 1;
        }
        if self.encoding_options.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.IngressVideoOptions", len)?;
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if self.source != 0 {
            let v = TrackSource::from_i32(self.source)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.source)))?;
            struct_ser.serialize_field("source", &v)?;
        }
        if let Some(v) = self.encoding_options.as_ref() {
            match v {
                ingress_video_options::EncodingOptions::Preset(v) => {
                    let v = IngressVideoEncodingPreset::from_i32(*v)
                        .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("preset", &v)?;
                }
                ingress_video_options::EncodingOptions::Options(v) => {
                    struct_ser.serialize_field("options", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IngressVideoOptions {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "name",
            "source",
            "preset",
            "options",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Name,
            Source,
            Preset,
            Options,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "name" => Ok(GeneratedField::Name),
                            "source" => Ok(GeneratedField::Source),
                            "preset" => Ok(GeneratedField::Preset),
                            "options" => Ok(GeneratedField::Options),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IngressVideoOptions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.IngressVideoOptions")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<IngressVideoOptions, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut name__ = None;
                let mut source__ = None;
                let mut encoding_options__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Source => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source__ = Some(map.next_value::<TrackSource>()? as i32);
                        }
                        GeneratedField::Preset => {
                            if encoding_options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preset"));
                            }
                            encoding_options__ = map.next_value::<::std::option::Option<IngressVideoEncodingPreset>>()?.map(|x| ingress_video_options::EncodingOptions::Preset(x as i32));
                        }
                        GeneratedField::Options => {
                            if encoding_options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("options"));
                            }
                            encoding_options__ = map.next_value::<::std::option::Option<_>>()?.map(ingress_video_options::EncodingOptions::Options)
;
                        }
                    }
                }
                Ok(IngressVideoOptions {
                    name: name__.unwrap_or_default(),
                    source: source__.unwrap_or_default(),
                    encoding_options: encoding_options__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.IngressVideoOptions", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InputAudioState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.mime_type.is_empty() {
            len += 1;
        }
        if self.average_bitrate != 0 {
            len += 1;
        }
        if self.channels != 0 {
            len += 1;
        }
        if self.sample_rate != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.InputAudioState", len)?;
        if !self.mime_type.is_empty() {
            struct_ser.serialize_field("mimeType", &self.mime_type)?;
        }
        if self.average_bitrate != 0 {
            struct_ser.serialize_field("averageBitrate", &self.average_bitrate)?;
        }
        if self.channels != 0 {
            struct_ser.serialize_field("channels", &self.channels)?;
        }
        if self.sample_rate != 0 {
            struct_ser.serialize_field("sampleRate", &self.sample_rate)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InputAudioState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "mime_type",
            "mimeType",
            "average_bitrate",
            "averageBitrate",
            "channels",
            "sample_rate",
            "sampleRate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MimeType,
            AverageBitrate,
            Channels,
            SampleRate,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "mimeType" | "mime_type" => Ok(GeneratedField::MimeType),
                            "averageBitrate" | "average_bitrate" => Ok(GeneratedField::AverageBitrate),
                            "channels" => Ok(GeneratedField::Channels),
                            "sampleRate" | "sample_rate" => Ok(GeneratedField::SampleRate),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InputAudioState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.InputAudioState")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<InputAudioState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut mime_type__ = None;
                let mut average_bitrate__ = None;
                let mut channels__ = None;
                let mut sample_rate__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::MimeType => {
                            if mime_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mimeType"));
                            }
                            mime_type__ = Some(map.next_value()?);
                        }
                        GeneratedField::AverageBitrate => {
                            if average_bitrate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("averageBitrate"));
                            }
                            average_bitrate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Channels => {
                            if channels__.is_some() {
                                return Err(serde::de::Error::duplicate_field("channels"));
                            }
                            channels__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SampleRate => {
                            if sample_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sampleRate"));
                            }
                            sample_rate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(InputAudioState {
                    mime_type: mime_type__.unwrap_or_default(),
                    average_bitrate: average_bitrate__.unwrap_or_default(),
                    channels: channels__.unwrap_or_default(),
                    sample_rate: sample_rate__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.InputAudioState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InputVideoState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.mime_type.is_empty() {
            len += 1;
        }
        if self.average_bitrate != 0 {
            len += 1;
        }
        if self.width != 0 {
            len += 1;
        }
        if self.height != 0 {
            len += 1;
        }
        if self.framerate != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.InputVideoState", len)?;
        if !self.mime_type.is_empty() {
            struct_ser.serialize_field("mimeType", &self.mime_type)?;
        }
        if self.average_bitrate != 0 {
            struct_ser.serialize_field("averageBitrate", &self.average_bitrate)?;
        }
        if self.width != 0 {
            struct_ser.serialize_field("width", &self.width)?;
        }
        if self.height != 0 {
            struct_ser.serialize_field("height", &self.height)?;
        }
        if self.framerate != 0. {
            struct_ser.serialize_field("framerate", &self.framerate)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InputVideoState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "mime_type",
            "mimeType",
            "average_bitrate",
            "averageBitrate",
            "width",
            "height",
            "framerate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MimeType,
            AverageBitrate,
            Width,
            Height,
            Framerate,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "mimeType" | "mime_type" => Ok(GeneratedField::MimeType),
                            "averageBitrate" | "average_bitrate" => Ok(GeneratedField::AverageBitrate),
                            "width" => Ok(GeneratedField::Width),
                            "height" => Ok(GeneratedField::Height),
                            "framerate" => Ok(GeneratedField::Framerate),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InputVideoState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.InputVideoState")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<InputVideoState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut mime_type__ = None;
                let mut average_bitrate__ = None;
                let mut width__ = None;
                let mut height__ = None;
                let mut framerate__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::MimeType => {
                            if mime_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mimeType"));
                            }
                            mime_type__ = Some(map.next_value()?);
                        }
                        GeneratedField::AverageBitrate => {
                            if average_bitrate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("averageBitrate"));
                            }
                            average_bitrate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Width => {
                            if width__.is_some() {
                                return Err(serde::de::Error::duplicate_field("width"));
                            }
                            width__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Framerate => {
                            if framerate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("framerate"));
                            }
                            framerate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(InputVideoState {
                    mime_type: mime_type__.unwrap_or_default(),
                    average_bitrate: average_bitrate__.unwrap_or_default(),
                    width: width__.unwrap_or_default(),
                    height: height__.unwrap_or_default(),
                    framerate: framerate__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.InputVideoState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for JoinResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.room.is_some() {
            len += 1;
        }
        if self.participant.is_some() {
            len += 1;
        }
        if !self.other_participants.is_empty() {
            len += 1;
        }
        if !self.server_version.is_empty() {
            len += 1;
        }
        if !self.ice_servers.is_empty() {
            len += 1;
        }
        if self.subscriber_primary {
            len += 1;
        }
        if !self.alternative_url.is_empty() {
            len += 1;
        }
        if self.client_configuration.is_some() {
            len += 1;
        }
        if !self.server_region.is_empty() {
            len += 1;
        }
        if self.ping_timeout != 0 {
            len += 1;
        }
        if self.ping_interval != 0 {
            len += 1;
        }
        if self.server_info.is_some() {
            len += 1;
        }
        if !self.sif_trailer.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.JoinResponse", len)?;
        if let Some(v) = self.room.as_ref() {
            struct_ser.serialize_field("room", v)?;
        }
        if let Some(v) = self.participant.as_ref() {
            struct_ser.serialize_field("participant", v)?;
        }
        if !self.other_participants.is_empty() {
            struct_ser.serialize_field("otherParticipants", &self.other_participants)?;
        }
        if !self.server_version.is_empty() {
            struct_ser.serialize_field("serverVersion", &self.server_version)?;
        }
        if !self.ice_servers.is_empty() {
            struct_ser.serialize_field("iceServers", &self.ice_servers)?;
        }
        if self.subscriber_primary {
            struct_ser.serialize_field("subscriberPrimary", &self.subscriber_primary)?;
        }
        if !self.alternative_url.is_empty() {
            struct_ser.serialize_field("alternativeUrl", &self.alternative_url)?;
        }
        if let Some(v) = self.client_configuration.as_ref() {
            struct_ser.serialize_field("clientConfiguration", v)?;
        }
        if !self.server_region.is_empty() {
            struct_ser.serialize_field("serverRegion", &self.server_region)?;
        }
        if self.ping_timeout != 0 {
            struct_ser.serialize_field("pingTimeout", &self.ping_timeout)?;
        }
        if self.ping_interval != 0 {
            struct_ser.serialize_field("pingInterval", &self.ping_interval)?;
        }
        if let Some(v) = self.server_info.as_ref() {
            struct_ser.serialize_field("serverInfo", v)?;
        }
        if !self.sif_trailer.is_empty() {
            struct_ser.serialize_field("sifTrailer", pbjson::private::base64::encode(&self.sif_trailer).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for JoinResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
            "participant",
            "other_participants",
            "otherParticipants",
            "server_version",
            "serverVersion",
            "ice_servers",
            "iceServers",
            "subscriber_primary",
            "subscriberPrimary",
            "alternative_url",
            "alternativeUrl",
            "client_configuration",
            "clientConfiguration",
            "server_region",
            "serverRegion",
            "ping_timeout",
            "pingTimeout",
            "ping_interval",
            "pingInterval",
            "server_info",
            "serverInfo",
            "sif_trailer",
            "sifTrailer",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
            Participant,
            OtherParticipants,
            ServerVersion,
            IceServers,
            SubscriberPrimary,
            AlternativeUrl,
            ClientConfiguration,
            ServerRegion,
            PingTimeout,
            PingInterval,
            ServerInfo,
            SifTrailer,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            "participant" => Ok(GeneratedField::Participant),
                            "otherParticipants" | "other_participants" => Ok(GeneratedField::OtherParticipants),
                            "serverVersion" | "server_version" => Ok(GeneratedField::ServerVersion),
                            "iceServers" | "ice_servers" => Ok(GeneratedField::IceServers),
                            "subscriberPrimary" | "subscriber_primary" => Ok(GeneratedField::SubscriberPrimary),
                            "alternativeUrl" | "alternative_url" => Ok(GeneratedField::AlternativeUrl),
                            "clientConfiguration" | "client_configuration" => Ok(GeneratedField::ClientConfiguration),
                            "serverRegion" | "server_region" => Ok(GeneratedField::ServerRegion),
                            "pingTimeout" | "ping_timeout" => Ok(GeneratedField::PingTimeout),
                            "pingInterval" | "ping_interval" => Ok(GeneratedField::PingInterval),
                            "serverInfo" | "server_info" => Ok(GeneratedField::ServerInfo),
                            "sifTrailer" | "sif_trailer" => Ok(GeneratedField::SifTrailer),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = JoinResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.JoinResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<JoinResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                let mut participant__ = None;
                let mut other_participants__ = None;
                let mut server_version__ = None;
                let mut ice_servers__ = None;
                let mut subscriber_primary__ = None;
                let mut alternative_url__ = None;
                let mut client_configuration__ = None;
                let mut server_region__ = None;
                let mut ping_timeout__ = None;
                let mut ping_interval__ = None;
                let mut server_info__ = None;
                let mut sif_trailer__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = map.next_value()?;
                        }
                        GeneratedField::Participant => {
                            if participant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participant"));
                            }
                            participant__ = map.next_value()?;
                        }
                        GeneratedField::OtherParticipants => {
                            if other_participants__.is_some() {
                                return Err(serde::de::Error::duplicate_field("otherParticipants"));
                            }
                            other_participants__ = Some(map.next_value()?);
                        }
                        GeneratedField::ServerVersion => {
                            if server_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverVersion"));
                            }
                            server_version__ = Some(map.next_value()?);
                        }
                        GeneratedField::IceServers => {
                            if ice_servers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("iceServers"));
                            }
                            ice_servers__ = Some(map.next_value()?);
                        }
                        GeneratedField::SubscriberPrimary => {
                            if subscriber_primary__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscriberPrimary"));
                            }
                            subscriber_primary__ = Some(map.next_value()?);
                        }
                        GeneratedField::AlternativeUrl => {
                            if alternative_url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("alternativeUrl"));
                            }
                            alternative_url__ = Some(map.next_value()?);
                        }
                        GeneratedField::ClientConfiguration => {
                            if client_configuration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("clientConfiguration"));
                            }
                            client_configuration__ = map.next_value()?;
                        }
                        GeneratedField::ServerRegion => {
                            if server_region__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverRegion"));
                            }
                            server_region__ = Some(map.next_value()?);
                        }
                        GeneratedField::PingTimeout => {
                            if ping_timeout__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pingTimeout"));
                            }
                            ping_timeout__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PingInterval => {
                            if ping_interval__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pingInterval"));
                            }
                            ping_interval__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ServerInfo => {
                            if server_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverInfo"));
                            }
                            server_info__ = map.next_value()?;
                        }
                        GeneratedField::SifTrailer => {
                            if sif_trailer__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sifTrailer"));
                            }
                            sif_trailer__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(JoinResponse {
                    room: room__,
                    participant: participant__,
                    other_participants: other_participants__.unwrap_or_default(),
                    server_version: server_version__.unwrap_or_default(),
                    ice_servers: ice_servers__.unwrap_or_default(),
                    subscriber_primary: subscriber_primary__.unwrap_or_default(),
                    alternative_url: alternative_url__.unwrap_or_default(),
                    client_configuration: client_configuration__,
                    server_region: server_region__.unwrap_or_default(),
                    ping_timeout: ping_timeout__.unwrap_or_default(),
                    ping_interval: ping_interval__.unwrap_or_default(),
                    server_info: server_info__,
                    sif_trailer: sif_trailer__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.JoinResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for LeaveRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.can_reconnect {
            len += 1;
        }
        if self.reason != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.LeaveRequest", len)?;
        if self.can_reconnect {
            struct_ser.serialize_field("canReconnect", &self.can_reconnect)?;
        }
        if self.reason != 0 {
            let v = DisconnectReason::from_i32(self.reason)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.reason)))?;
            struct_ser.serialize_field("reason", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for LeaveRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "can_reconnect",
            "canReconnect",
            "reason",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CanReconnect,
            Reason,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "canReconnect" | "can_reconnect" => Ok(GeneratedField::CanReconnect),
                            "reason" => Ok(GeneratedField::Reason),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = LeaveRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.LeaveRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<LeaveRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut can_reconnect__ = None;
                let mut reason__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::CanReconnect => {
                            if can_reconnect__.is_some() {
                                return Err(serde::de::Error::duplicate_field("canReconnect"));
                            }
                            can_reconnect__ = Some(map.next_value()?);
                        }
                        GeneratedField::Reason => {
                            if reason__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reason"));
                            }
                            reason__ = Some(map.next_value::<DisconnectReason>()? as i32);
                        }
                    }
                }
                Ok(LeaveRequest {
                    can_reconnect: can_reconnect__.unwrap_or_default(),
                    reason: reason__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.LeaveRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ListEgressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room_name.is_empty() {
            len += 1;
        }
        if !self.egress_id.is_empty() {
            len += 1;
        }
        if self.active {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ListEgressRequest", len)?;
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if !self.egress_id.is_empty() {
            struct_ser.serialize_field("egressId", &self.egress_id)?;
        }
        if self.active {
            struct_ser.serialize_field("active", &self.active)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ListEgressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room_name",
            "roomName",
            "egress_id",
            "egressId",
            "active",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RoomName,
            EgressId,
            Active,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "egressId" | "egress_id" => Ok(GeneratedField::EgressId),
                            "active" => Ok(GeneratedField::Active),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ListEgressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ListEgressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ListEgressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room_name__ = None;
                let mut egress_id__ = None;
                let mut active__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::EgressId => {
                            if egress_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("egressId"));
                            }
                            egress_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::Active => {
                            if active__.is_some() {
                                return Err(serde::de::Error::duplicate_field("active"));
                            }
                            active__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ListEgressRequest {
                    room_name: room_name__.unwrap_or_default(),
                    egress_id: egress_id__.unwrap_or_default(),
                    active: active__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ListEgressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ListEgressResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.items.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ListEgressResponse", len)?;
        if !self.items.is_empty() {
            struct_ser.serialize_field("items", &self.items)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ListEgressResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "items",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Items,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "items" => Ok(GeneratedField::Items),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ListEgressResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ListEgressResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ListEgressResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut items__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Items => {
                            if items__.is_some() {
                                return Err(serde::de::Error::duplicate_field("items"));
                            }
                            items__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ListEgressResponse {
                    items: items__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ListEgressResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ListIngressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room_name.is_empty() {
            len += 1;
        }
        if !self.ingress_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ListIngressRequest", len)?;
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if !self.ingress_id.is_empty() {
            struct_ser.serialize_field("ingressId", &self.ingress_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ListIngressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room_name",
            "roomName",
            "ingress_id",
            "ingressId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RoomName,
            IngressId,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "ingressId" | "ingress_id" => Ok(GeneratedField::IngressId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ListIngressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ListIngressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ListIngressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room_name__ = None;
                let mut ingress_id__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::IngressId => {
                            if ingress_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ingressId"));
                            }
                            ingress_id__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ListIngressRequest {
                    room_name: room_name__.unwrap_or_default(),
                    ingress_id: ingress_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ListIngressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ListIngressResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.items.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ListIngressResponse", len)?;
        if !self.items.is_empty() {
            struct_ser.serialize_field("items", &self.items)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ListIngressResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "items",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Items,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "items" => Ok(GeneratedField::Items),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ListIngressResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ListIngressResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ListIngressResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut items__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Items => {
                            if items__.is_some() {
                                return Err(serde::de::Error::duplicate_field("items"));
                            }
                            items__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ListIngressResponse {
                    items: items__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ListIngressResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ListParticipantsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ListParticipantsRequest", len)?;
        if !self.room.is_empty() {
            struct_ser.serialize_field("room", &self.room)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ListParticipantsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ListParticipantsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ListParticipantsRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ListParticipantsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ListParticipantsRequest {
                    room: room__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ListParticipantsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ListParticipantsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.participants.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ListParticipantsResponse", len)?;
        if !self.participants.is_empty() {
            struct_ser.serialize_field("participants", &self.participants)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ListParticipantsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "participants",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Participants,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "participants" => Ok(GeneratedField::Participants),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ListParticipantsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ListParticipantsResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ListParticipantsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut participants__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Participants => {
                            if participants__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participants"));
                            }
                            participants__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ListParticipantsResponse {
                    participants: participants__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ListParticipantsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ListRoomsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.names.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ListRoomsRequest", len)?;
        if !self.names.is_empty() {
            struct_ser.serialize_field("names", &self.names)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ListRoomsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "names",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Names,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "names" => Ok(GeneratedField::Names),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ListRoomsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ListRoomsRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ListRoomsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut names__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Names => {
                            if names__.is_some() {
                                return Err(serde::de::Error::duplicate_field("names"));
                            }
                            names__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ListRoomsRequest {
                    names: names__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ListRoomsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ListRoomsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.rooms.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ListRoomsResponse", len)?;
        if !self.rooms.is_empty() {
            struct_ser.serialize_field("rooms", &self.rooms)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ListRoomsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "rooms",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Rooms,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "rooms" => Ok(GeneratedField::Rooms),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ListRoomsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ListRoomsResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ListRoomsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut rooms__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Rooms => {
                            if rooms__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rooms"));
                            }
                            rooms__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ListRoomsResponse {
                    rooms: rooms__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ListRoomsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MuteRoomTrackRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room.is_empty() {
            len += 1;
        }
        if !self.identity.is_empty() {
            len += 1;
        }
        if !self.track_sid.is_empty() {
            len += 1;
        }
        if self.muted {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.MuteRoomTrackRequest", len)?;
        if !self.room.is_empty() {
            struct_ser.serialize_field("room", &self.room)?;
        }
        if !self.identity.is_empty() {
            struct_ser.serialize_field("identity", &self.identity)?;
        }
        if !self.track_sid.is_empty() {
            struct_ser.serialize_field("trackSid", &self.track_sid)?;
        }
        if self.muted {
            struct_ser.serialize_field("muted", &self.muted)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MuteRoomTrackRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
            "identity",
            "track_sid",
            "trackSid",
            "muted",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
            Identity,
            TrackSid,
            Muted,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            "identity" => Ok(GeneratedField::Identity),
                            "trackSid" | "track_sid" => Ok(GeneratedField::TrackSid),
                            "muted" => Ok(GeneratedField::Muted),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MuteRoomTrackRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.MuteRoomTrackRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<MuteRoomTrackRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                let mut identity__ = None;
                let mut track_sid__ = None;
                let mut muted__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = Some(map.next_value()?);
                        }
                        GeneratedField::Identity => {
                            if identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identity"));
                            }
                            identity__ = Some(map.next_value()?);
                        }
                        GeneratedField::TrackSid => {
                            if track_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSid"));
                            }
                            track_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Muted => {
                            if muted__.is_some() {
                                return Err(serde::de::Error::duplicate_field("muted"));
                            }
                            muted__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(MuteRoomTrackRequest {
                    room: room__.unwrap_or_default(),
                    identity: identity__.unwrap_or_default(),
                    track_sid: track_sid__.unwrap_or_default(),
                    muted: muted__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.MuteRoomTrackRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MuteRoomTrackResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.track.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.MuteRoomTrackResponse", len)?;
        if let Some(v) = self.track.as_ref() {
            struct_ser.serialize_field("track", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MuteRoomTrackResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "track",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Track,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "track" => Ok(GeneratedField::Track),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MuteRoomTrackResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.MuteRoomTrackResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<MuteRoomTrackResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut track__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Track => {
                            if track__.is_some() {
                                return Err(serde::de::Error::duplicate_field("track"));
                            }
                            track__ = map.next_value()?;
                        }
                    }
                }
                Ok(MuteRoomTrackResponse {
                    track: track__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.MuteRoomTrackResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MuteTrackRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.sid.is_empty() {
            len += 1;
        }
        if self.muted {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.MuteTrackRequest", len)?;
        if !self.sid.is_empty() {
            struct_ser.serialize_field("sid", &self.sid)?;
        }
        if self.muted {
            struct_ser.serialize_field("muted", &self.muted)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MuteTrackRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sid",
            "muted",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Sid,
            Muted,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "sid" => Ok(GeneratedField::Sid),
                            "muted" => Ok(GeneratedField::Muted),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MuteTrackRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.MuteTrackRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<MuteTrackRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sid__ = None;
                let mut muted__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Sid => {
                            if sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sid"));
                            }
                            sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Muted => {
                            if muted__.is_some() {
                                return Err(serde::de::Error::duplicate_field("muted"));
                            }
                            muted__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(MuteTrackRequest {
                    sid: sid__.unwrap_or_default(),
                    muted: muted__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.MuteTrackRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ParticipantEgressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room_name.is_empty() {
            len += 1;
        }
        if !self.identity.is_empty() {
            len += 1;
        }
        if self.screen_share {
            len += 1;
        }
        if !self.file_outputs.is_empty() {
            len += 1;
        }
        if !self.stream_outputs.is_empty() {
            len += 1;
        }
        if !self.segment_outputs.is_empty() {
            len += 1;
        }
        if !self.image_outputs.is_empty() {
            len += 1;
        }
        if self.options.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ParticipantEgressRequest", len)?;
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if !self.identity.is_empty() {
            struct_ser.serialize_field("identity", &self.identity)?;
        }
        if self.screen_share {
            struct_ser.serialize_field("screenShare", &self.screen_share)?;
        }
        if !self.file_outputs.is_empty() {
            struct_ser.serialize_field("fileOutputs", &self.file_outputs)?;
        }
        if !self.stream_outputs.is_empty() {
            struct_ser.serialize_field("streamOutputs", &self.stream_outputs)?;
        }
        if !self.segment_outputs.is_empty() {
            struct_ser.serialize_field("segmentOutputs", &self.segment_outputs)?;
        }
        if !self.image_outputs.is_empty() {
            struct_ser.serialize_field("imageOutputs", &self.image_outputs)?;
        }
        if let Some(v) = self.options.as_ref() {
            match v {
                participant_egress_request::Options::Preset(v) => {
                    let v = EncodingOptionsPreset::from_i32(*v)
                        .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("preset", &v)?;
                }
                participant_egress_request::Options::Advanced(v) => {
                    struct_ser.serialize_field("advanced", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ParticipantEgressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room_name",
            "roomName",
            "identity",
            "screen_share",
            "screenShare",
            "file_outputs",
            "fileOutputs",
            "stream_outputs",
            "streamOutputs",
            "segment_outputs",
            "segmentOutputs",
            "image_outputs",
            "imageOutputs",
            "preset",
            "advanced",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RoomName,
            Identity,
            ScreenShare,
            FileOutputs,
            StreamOutputs,
            SegmentOutputs,
            ImageOutputs,
            Preset,
            Advanced,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "identity" => Ok(GeneratedField::Identity),
                            "screenShare" | "screen_share" => Ok(GeneratedField::ScreenShare),
                            "fileOutputs" | "file_outputs" => Ok(GeneratedField::FileOutputs),
                            "streamOutputs" | "stream_outputs" => Ok(GeneratedField::StreamOutputs),
                            "segmentOutputs" | "segment_outputs" => Ok(GeneratedField::SegmentOutputs),
                            "imageOutputs" | "image_outputs" => Ok(GeneratedField::ImageOutputs),
                            "preset" => Ok(GeneratedField::Preset),
                            "advanced" => Ok(GeneratedField::Advanced),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ParticipantEgressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ParticipantEgressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ParticipantEgressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room_name__ = None;
                let mut identity__ = None;
                let mut screen_share__ = None;
                let mut file_outputs__ = None;
                let mut stream_outputs__ = None;
                let mut segment_outputs__ = None;
                let mut image_outputs__ = None;
                let mut options__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Identity => {
                            if identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identity"));
                            }
                            identity__ = Some(map.next_value()?);
                        }
                        GeneratedField::ScreenShare => {
                            if screen_share__.is_some() {
                                return Err(serde::de::Error::duplicate_field("screenShare"));
                            }
                            screen_share__ = Some(map.next_value()?);
                        }
                        GeneratedField::FileOutputs => {
                            if file_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fileOutputs"));
                            }
                            file_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::StreamOutputs => {
                            if stream_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("streamOutputs"));
                            }
                            stream_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::SegmentOutputs => {
                            if segment_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segmentOutputs"));
                            }
                            segment_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::ImageOutputs => {
                            if image_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("imageOutputs"));
                            }
                            image_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::Preset => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preset"));
                            }
                            options__ = map.next_value::<::std::option::Option<EncodingOptionsPreset>>()?.map(|x| participant_egress_request::Options::Preset(x as i32));
                        }
                        GeneratedField::Advanced => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("advanced"));
                            }
                            options__ = map.next_value::<::std::option::Option<_>>()?.map(participant_egress_request::Options::Advanced)
;
                        }
                    }
                }
                Ok(ParticipantEgressRequest {
                    room_name: room_name__.unwrap_or_default(),
                    identity: identity__.unwrap_or_default(),
                    screen_share: screen_share__.unwrap_or_default(),
                    file_outputs: file_outputs__.unwrap_or_default(),
                    stream_outputs: stream_outputs__.unwrap_or_default(),
                    segment_outputs: segment_outputs__.unwrap_or_default(),
                    image_outputs: image_outputs__.unwrap_or_default(),
                    options: options__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.ParticipantEgressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ParticipantInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.sid.is_empty() {
            len += 1;
        }
        if !self.identity.is_empty() {
            len += 1;
        }
        if self.state != 0 {
            len += 1;
        }
        if !self.tracks.is_empty() {
            len += 1;
        }
        if !self.metadata.is_empty() {
            len += 1;
        }
        if self.joined_at != 0 {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        if self.version != 0 {
            len += 1;
        }
        if self.permission.is_some() {
            len += 1;
        }
        if !self.region.is_empty() {
            len += 1;
        }
        if self.is_publisher {
            len += 1;
        }
        if self.kind != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ParticipantInfo", len)?;
        if !self.sid.is_empty() {
            struct_ser.serialize_field("sid", &self.sid)?;
        }
        if !self.identity.is_empty() {
            struct_ser.serialize_field("identity", &self.identity)?;
        }
        if self.state != 0 {
            let v = participant_info::State::from_i32(self.state)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.state)))?;
            struct_ser.serialize_field("state", &v)?;
        }
        if !self.tracks.is_empty() {
            struct_ser.serialize_field("tracks", &self.tracks)?;
        }
        if !self.metadata.is_empty() {
            struct_ser.serialize_field("metadata", &self.metadata)?;
        }
        if self.joined_at != 0 {
            struct_ser.serialize_field("joinedAt", ToString::to_string(&self.joined_at).as_str())?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if self.version != 0 {
            struct_ser.serialize_field("version", &self.version)?;
        }
        if let Some(v) = self.permission.as_ref() {
            struct_ser.serialize_field("permission", v)?;
        }
        if !self.region.is_empty() {
            struct_ser.serialize_field("region", &self.region)?;
        }
        if self.is_publisher {
            struct_ser.serialize_field("isPublisher", &self.is_publisher)?;
        }
        if self.kind != 0 {
            let v = participant_info::Kind::from_i32(self.kind)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.kind)))?;
            struct_ser.serialize_field("kind", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ParticipantInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sid",
            "identity",
            "state",
            "tracks",
            "metadata",
            "joined_at",
            "joinedAt",
            "name",
            "version",
            "permission",
            "region",
            "is_publisher",
            "isPublisher",
            "kind",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Sid,
            Identity,
            State,
            Tracks,
            Metadata,
            JoinedAt,
            Name,
            Version,
            Permission,
            Region,
            IsPublisher,
            Kind,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "sid" => Ok(GeneratedField::Sid),
                            "identity" => Ok(GeneratedField::Identity),
                            "state" => Ok(GeneratedField::State),
                            "tracks" => Ok(GeneratedField::Tracks),
                            "metadata" => Ok(GeneratedField::Metadata),
                            "joinedAt" | "joined_at" => Ok(GeneratedField::JoinedAt),
                            "name" => Ok(GeneratedField::Name),
                            "version" => Ok(GeneratedField::Version),
                            "permission" => Ok(GeneratedField::Permission),
                            "region" => Ok(GeneratedField::Region),
                            "isPublisher" | "is_publisher" => Ok(GeneratedField::IsPublisher),
                            "kind" => Ok(GeneratedField::Kind),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ParticipantInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ParticipantInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ParticipantInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sid__ = None;
                let mut identity__ = None;
                let mut state__ = None;
                let mut tracks__ = None;
                let mut metadata__ = None;
                let mut joined_at__ = None;
                let mut name__ = None;
                let mut version__ = None;
                let mut permission__ = None;
                let mut region__ = None;
                let mut is_publisher__ = None;
                let mut kind__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Sid => {
                            if sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sid"));
                            }
                            sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Identity => {
                            if identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identity"));
                            }
                            identity__ = Some(map.next_value()?);
                        }
                        GeneratedField::State => {
                            if state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("state"));
                            }
                            state__ = Some(map.next_value::<participant_info::State>()? as i32);
                        }
                        GeneratedField::Tracks => {
                            if tracks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tracks"));
                            }
                            tracks__ = Some(map.next_value()?);
                        }
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = Some(map.next_value()?);
                        }
                        GeneratedField::JoinedAt => {
                            if joined_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("joinedAt"));
                            }
                            joined_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Version => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("version"));
                            }
                            version__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Permission => {
                            if permission__.is_some() {
                                return Err(serde::de::Error::duplicate_field("permission"));
                            }
                            permission__ = map.next_value()?;
                        }
                        GeneratedField::Region => {
                            if region__.is_some() {
                                return Err(serde::de::Error::duplicate_field("region"));
                            }
                            region__ = Some(map.next_value()?);
                        }
                        GeneratedField::IsPublisher => {
                            if is_publisher__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isPublisher"));
                            }
                            is_publisher__ = Some(map.next_value()?);
                        }
                        GeneratedField::Kind => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("kind"));
                            }
                            kind__ = Some(map.next_value::<participant_info::Kind>()? as i32);
                        }
                    }
                }
                Ok(ParticipantInfo {
                    sid: sid__.unwrap_or_default(),
                    identity: identity__.unwrap_or_default(),
                    state: state__.unwrap_or_default(),
                    tracks: tracks__.unwrap_or_default(),
                    metadata: metadata__.unwrap_or_default(),
                    joined_at: joined_at__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                    version: version__.unwrap_or_default(),
                    permission: permission__,
                    region: region__.unwrap_or_default(),
                    is_publisher: is_publisher__.unwrap_or_default(),
                    kind: kind__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ParticipantInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for participant_info::Kind {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Standard => "STANDARD",
            Self::Ingress => "INGRESS",
            Self::Egress => "EGRESS",
            Self::Sip => "SIP",
            Self::Agent => "AGENT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for participant_info::Kind {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "STANDARD",
            "INGRESS",
            "EGRESS",
            "SIP",
            "AGENT",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = participant_info::Kind;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(participant_info::Kind::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(participant_info::Kind::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "STANDARD" => Ok(participant_info::Kind::Standard),
                    "INGRESS" => Ok(participant_info::Kind::Ingress),
                    "EGRESS" => Ok(participant_info::Kind::Egress),
                    "SIP" => Ok(participant_info::Kind::Sip),
                    "AGENT" => Ok(participant_info::Kind::Agent),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for participant_info::State {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Joining => "JOINING",
            Self::Joined => "JOINED",
            Self::Active => "ACTIVE",
            Self::Disconnected => "DISCONNECTED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for participant_info::State {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "JOINING",
            "JOINED",
            "ACTIVE",
            "DISCONNECTED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = participant_info::State;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(participant_info::State::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(participant_info::State::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "JOINING" => Ok(participant_info::State::Joining),
                    "JOINED" => Ok(participant_info::State::Joined),
                    "ACTIVE" => Ok(participant_info::State::Active),
                    "DISCONNECTED" => Ok(participant_info::State::Disconnected),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ParticipantPermission {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.can_subscribe {
            len += 1;
        }
        if self.can_publish {
            len += 1;
        }
        if self.can_publish_data {
            len += 1;
        }
        if !self.can_publish_sources.is_empty() {
            len += 1;
        }
        if self.hidden {
            len += 1;
        }
        if self.recorder {
            len += 1;
        }
        if self.can_update_metadata {
            len += 1;
        }
        if self.agent {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ParticipantPermission", len)?;
        if self.can_subscribe {
            struct_ser.serialize_field("canSubscribe", &self.can_subscribe)?;
        }
        if self.can_publish {
            struct_ser.serialize_field("canPublish", &self.can_publish)?;
        }
        if self.can_publish_data {
            struct_ser.serialize_field("canPublishData", &self.can_publish_data)?;
        }
        if !self.can_publish_sources.is_empty() {
            let v = self.can_publish_sources.iter().cloned().map(|v| {
                TrackSource::from_i32(v)
                    .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", v)))
                }).collect::<Result<Vec<_>, _>>()?;
            struct_ser.serialize_field("canPublishSources", &v)?;
        }
        if self.hidden {
            struct_ser.serialize_field("hidden", &self.hidden)?;
        }
        if self.recorder {
            struct_ser.serialize_field("recorder", &self.recorder)?;
        }
        if self.can_update_metadata {
            struct_ser.serialize_field("canUpdateMetadata", &self.can_update_metadata)?;
        }
        if self.agent {
            struct_ser.serialize_field("agent", &self.agent)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ParticipantPermission {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "can_subscribe",
            "canSubscribe",
            "can_publish",
            "canPublish",
            "can_publish_data",
            "canPublishData",
            "can_publish_sources",
            "canPublishSources",
            "hidden",
            "recorder",
            "can_update_metadata",
            "canUpdateMetadata",
            "agent",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CanSubscribe,
            CanPublish,
            CanPublishData,
            CanPublishSources,
            Hidden,
            Recorder,
            CanUpdateMetadata,
            Agent,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "canSubscribe" | "can_subscribe" => Ok(GeneratedField::CanSubscribe),
                            "canPublish" | "can_publish" => Ok(GeneratedField::CanPublish),
                            "canPublishData" | "can_publish_data" => Ok(GeneratedField::CanPublishData),
                            "canPublishSources" | "can_publish_sources" => Ok(GeneratedField::CanPublishSources),
                            "hidden" => Ok(GeneratedField::Hidden),
                            "recorder" => Ok(GeneratedField::Recorder),
                            "canUpdateMetadata" | "can_update_metadata" => Ok(GeneratedField::CanUpdateMetadata),
                            "agent" => Ok(GeneratedField::Agent),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ParticipantPermission;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ParticipantPermission")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ParticipantPermission, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut can_subscribe__ = None;
                let mut can_publish__ = None;
                let mut can_publish_data__ = None;
                let mut can_publish_sources__ = None;
                let mut hidden__ = None;
                let mut recorder__ = None;
                let mut can_update_metadata__ = None;
                let mut agent__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::CanSubscribe => {
                            if can_subscribe__.is_some() {
                                return Err(serde::de::Error::duplicate_field("canSubscribe"));
                            }
                            can_subscribe__ = Some(map.next_value()?);
                        }
                        GeneratedField::CanPublish => {
                            if can_publish__.is_some() {
                                return Err(serde::de::Error::duplicate_field("canPublish"));
                            }
                            can_publish__ = Some(map.next_value()?);
                        }
                        GeneratedField::CanPublishData => {
                            if can_publish_data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("canPublishData"));
                            }
                            can_publish_data__ = Some(map.next_value()?);
                        }
                        GeneratedField::CanPublishSources => {
                            if can_publish_sources__.is_some() {
                                return Err(serde::de::Error::duplicate_field("canPublishSources"));
                            }
                            can_publish_sources__ = Some(map.next_value::<Vec<TrackSource>>()?.into_iter().map(|x| x as i32).collect());
                        }
                        GeneratedField::Hidden => {
                            if hidden__.is_some() {
                                return Err(serde::de::Error::duplicate_field("hidden"));
                            }
                            hidden__ = Some(map.next_value()?);
                        }
                        GeneratedField::Recorder => {
                            if recorder__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recorder"));
                            }
                            recorder__ = Some(map.next_value()?);
                        }
                        GeneratedField::CanUpdateMetadata => {
                            if can_update_metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("canUpdateMetadata"));
                            }
                            can_update_metadata__ = Some(map.next_value()?);
                        }
                        GeneratedField::Agent => {
                            if agent__.is_some() {
                                return Err(serde::de::Error::duplicate_field("agent"));
                            }
                            agent__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ParticipantPermission {
                    can_subscribe: can_subscribe__.unwrap_or_default(),
                    can_publish: can_publish__.unwrap_or_default(),
                    can_publish_data: can_publish_data__.unwrap_or_default(),
                    can_publish_sources: can_publish_sources__.unwrap_or_default(),
                    hidden: hidden__.unwrap_or_default(),
                    recorder: recorder__.unwrap_or_default(),
                    can_update_metadata: can_update_metadata__.unwrap_or_default(),
                    agent: agent__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ParticipantPermission", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ParticipantTracks {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.participant_sid.is_empty() {
            len += 1;
        }
        if !self.track_sids.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ParticipantTracks", len)?;
        if !self.participant_sid.is_empty() {
            struct_ser.serialize_field("participantSid", &self.participant_sid)?;
        }
        if !self.track_sids.is_empty() {
            struct_ser.serialize_field("trackSids", &self.track_sids)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ParticipantTracks {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "participant_sid",
            "participantSid",
            "track_sids",
            "trackSids",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ParticipantSid,
            TrackSids,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "participantSid" | "participant_sid" => Ok(GeneratedField::ParticipantSid),
                            "trackSids" | "track_sids" => Ok(GeneratedField::TrackSids),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ParticipantTracks;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ParticipantTracks")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ParticipantTracks, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut participant_sid__ = None;
                let mut track_sids__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::ParticipantSid => {
                            if participant_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantSid"));
                            }
                            participant_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::TrackSids => {
                            if track_sids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSids"));
                            }
                            track_sids__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ParticipantTracks {
                    participant_sid: participant_sid__.unwrap_or_default(),
                    track_sids: track_sids__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ParticipantTracks", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ParticipantUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.participants.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ParticipantUpdate", len)?;
        if !self.participants.is_empty() {
            struct_ser.serialize_field("participants", &self.participants)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ParticipantUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "participants",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Participants,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "participants" => Ok(GeneratedField::Participants),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ParticipantUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ParticipantUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ParticipantUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut participants__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Participants => {
                            if participants__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participants"));
                            }
                            participants__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ParticipantUpdate {
                    participants: participants__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ParticipantUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Ping {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.timestamp != 0 {
            len += 1;
        }
        if self.rtt != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.Ping", len)?;
        if self.timestamp != 0 {
            struct_ser.serialize_field("timestamp", ToString::to_string(&self.timestamp).as_str())?;
        }
        if self.rtt != 0 {
            struct_ser.serialize_field("rtt", ToString::to_string(&self.rtt).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Ping {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "timestamp",
            "rtt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Timestamp,
            Rtt,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "timestamp" => Ok(GeneratedField::Timestamp),
                            "rtt" => Ok(GeneratedField::Rtt),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Ping;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.Ping")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<Ping, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut timestamp__ = None;
                let mut rtt__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Timestamp => {
                            if timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestamp"));
                            }
                            timestamp__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Rtt => {
                            if rtt__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rtt"));
                            }
                            rtt__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(Ping {
                    timestamp: timestamp__.unwrap_or_default(),
                    rtt: rtt__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.Ping", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PlayoutDelay {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.enabled {
            len += 1;
        }
        if self.min != 0 {
            len += 1;
        }
        if self.max != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.PlayoutDelay", len)?;
        if self.enabled {
            struct_ser.serialize_field("enabled", &self.enabled)?;
        }
        if self.min != 0 {
            struct_ser.serialize_field("min", &self.min)?;
        }
        if self.max != 0 {
            struct_ser.serialize_field("max", &self.max)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PlayoutDelay {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "enabled",
            "min",
            "max",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Enabled,
            Min,
            Max,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "enabled" => Ok(GeneratedField::Enabled),
                            "min" => Ok(GeneratedField::Min),
                            "max" => Ok(GeneratedField::Max),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PlayoutDelay;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.PlayoutDelay")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<PlayoutDelay, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut enabled__ = None;
                let mut min__ = None;
                let mut max__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Enabled => {
                            if enabled__.is_some() {
                                return Err(serde::de::Error::duplicate_field("enabled"));
                            }
                            enabled__ = Some(map.next_value()?);
                        }
                        GeneratedField::Min => {
                            if min__.is_some() {
                                return Err(serde::de::Error::duplicate_field("min"));
                            }
                            min__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Max => {
                            if max__.is_some() {
                                return Err(serde::de::Error::duplicate_field("max"));
                            }
                            max__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(PlayoutDelay {
                    enabled: enabled__.unwrap_or_default(),
                    min: min__.unwrap_or_default(),
                    max: max__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.PlayoutDelay", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Pong {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.last_ping_timestamp != 0 {
            len += 1;
        }
        if self.timestamp != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.Pong", len)?;
        if self.last_ping_timestamp != 0 {
            struct_ser.serialize_field("lastPingTimestamp", ToString::to_string(&self.last_ping_timestamp).as_str())?;
        }
        if self.timestamp != 0 {
            struct_ser.serialize_field("timestamp", ToString::to_string(&self.timestamp).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Pong {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "last_ping_timestamp",
            "lastPingTimestamp",
            "timestamp",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            LastPingTimestamp,
            Timestamp,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "lastPingTimestamp" | "last_ping_timestamp" => Ok(GeneratedField::LastPingTimestamp),
                            "timestamp" => Ok(GeneratedField::Timestamp),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Pong;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.Pong")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<Pong, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut last_ping_timestamp__ = None;
                let mut timestamp__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::LastPingTimestamp => {
                            if last_ping_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastPingTimestamp"));
                            }
                            last_ping_timestamp__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Timestamp => {
                            if timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestamp"));
                            }
                            timestamp__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(Pong {
                    last_ping_timestamp: last_ping_timestamp__.unwrap_or_default(),
                    timestamp: timestamp__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.Pong", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RtpDrift {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.start_time.is_some() {
            len += 1;
        }
        if self.end_time.is_some() {
            len += 1;
        }
        if self.duration != 0. {
            len += 1;
        }
        if self.start_timestamp != 0 {
            len += 1;
        }
        if self.end_timestamp != 0 {
            len += 1;
        }
        if self.rtp_clock_ticks != 0 {
            len += 1;
        }
        if self.drift_samples != 0 {
            len += 1;
        }
        if self.drift_ms != 0. {
            len += 1;
        }
        if self.clock_rate != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.RTPDrift", len)?;
        if let Some(v) = self.start_time.as_ref() {
            struct_ser.serialize_field("startTime", v)?;
        }
        if let Some(v) = self.end_time.as_ref() {
            struct_ser.serialize_field("endTime", v)?;
        }
        if self.duration != 0. {
            struct_ser.serialize_field("duration", &self.duration)?;
        }
        if self.start_timestamp != 0 {
            struct_ser.serialize_field("startTimestamp", ToString::to_string(&self.start_timestamp).as_str())?;
        }
        if self.end_timestamp != 0 {
            struct_ser.serialize_field("endTimestamp", ToString::to_string(&self.end_timestamp).as_str())?;
        }
        if self.rtp_clock_ticks != 0 {
            struct_ser.serialize_field("rtpClockTicks", ToString::to_string(&self.rtp_clock_ticks).as_str())?;
        }
        if self.drift_samples != 0 {
            struct_ser.serialize_field("driftSamples", ToString::to_string(&self.drift_samples).as_str())?;
        }
        if self.drift_ms != 0. {
            struct_ser.serialize_field("driftMs", &self.drift_ms)?;
        }
        if self.clock_rate != 0. {
            struct_ser.serialize_field("clockRate", &self.clock_rate)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RtpDrift {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "start_time",
            "startTime",
            "end_time",
            "endTime",
            "duration",
            "start_timestamp",
            "startTimestamp",
            "end_timestamp",
            "endTimestamp",
            "rtp_clock_ticks",
            "rtpClockTicks",
            "drift_samples",
            "driftSamples",
            "drift_ms",
            "driftMs",
            "clock_rate",
            "clockRate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            StartTime,
            EndTime,
            Duration,
            StartTimestamp,
            EndTimestamp,
            RtpClockTicks,
            DriftSamples,
            DriftMs,
            ClockRate,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "startTime" | "start_time" => Ok(GeneratedField::StartTime),
                            "endTime" | "end_time" => Ok(GeneratedField::EndTime),
                            "duration" => Ok(GeneratedField::Duration),
                            "startTimestamp" | "start_timestamp" => Ok(GeneratedField::StartTimestamp),
                            "endTimestamp" | "end_timestamp" => Ok(GeneratedField::EndTimestamp),
                            "rtpClockTicks" | "rtp_clock_ticks" => Ok(GeneratedField::RtpClockTicks),
                            "driftSamples" | "drift_samples" => Ok(GeneratedField::DriftSamples),
                            "driftMs" | "drift_ms" => Ok(GeneratedField::DriftMs),
                            "clockRate" | "clock_rate" => Ok(GeneratedField::ClockRate),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RtpDrift;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.RTPDrift")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RtpDrift, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut start_time__ = None;
                let mut end_time__ = None;
                let mut duration__ = None;
                let mut start_timestamp__ = None;
                let mut end_timestamp__ = None;
                let mut rtp_clock_ticks__ = None;
                let mut drift_samples__ = None;
                let mut drift_ms__ = None;
                let mut clock_rate__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::StartTime => {
                            if start_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startTime"));
                            }
                            start_time__ = map.next_value()?;
                        }
                        GeneratedField::EndTime => {
                            if end_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endTime"));
                            }
                            end_time__ = map.next_value()?;
                        }
                        GeneratedField::Duration => {
                            if duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("duration"));
                            }
                            duration__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::StartTimestamp => {
                            if start_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startTimestamp"));
                            }
                            start_timestamp__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndTimestamp => {
                            if end_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endTimestamp"));
                            }
                            end_timestamp__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RtpClockTicks => {
                            if rtp_clock_ticks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rtpClockTicks"));
                            }
                            rtp_clock_ticks__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DriftSamples => {
                            if drift_samples__.is_some() {
                                return Err(serde::de::Error::duplicate_field("driftSamples"));
                            }
                            drift_samples__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DriftMs => {
                            if drift_ms__.is_some() {
                                return Err(serde::de::Error::duplicate_field("driftMs"));
                            }
                            drift_ms__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ClockRate => {
                            if clock_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("clockRate"));
                            }
                            clock_rate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(RtpDrift {
                    start_time: start_time__,
                    end_time: end_time__,
                    duration: duration__.unwrap_or_default(),
                    start_timestamp: start_timestamp__.unwrap_or_default(),
                    end_timestamp: end_timestamp__.unwrap_or_default(),
                    rtp_clock_ticks: rtp_clock_ticks__.unwrap_or_default(),
                    drift_samples: drift_samples__.unwrap_or_default(),
                    drift_ms: drift_ms__.unwrap_or_default(),
                    clock_rate: clock_rate__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.RTPDrift", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RtpStats {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.start_time.is_some() {
            len += 1;
        }
        if self.end_time.is_some() {
            len += 1;
        }
        if self.duration != 0. {
            len += 1;
        }
        if self.packets != 0 {
            len += 1;
        }
        if self.packet_rate != 0. {
            len += 1;
        }
        if self.bytes != 0 {
            len += 1;
        }
        if self.header_bytes != 0 {
            len += 1;
        }
        if self.bitrate != 0. {
            len += 1;
        }
        if self.packets_lost != 0 {
            len += 1;
        }
        if self.packet_loss_rate != 0. {
            len += 1;
        }
        if self.packet_loss_percentage != 0. {
            len += 1;
        }
        if self.packets_duplicate != 0 {
            len += 1;
        }
        if self.packet_duplicate_rate != 0. {
            len += 1;
        }
        if self.bytes_duplicate != 0 {
            len += 1;
        }
        if self.header_bytes_duplicate != 0 {
            len += 1;
        }
        if self.bitrate_duplicate != 0. {
            len += 1;
        }
        if self.packets_padding != 0 {
            len += 1;
        }
        if self.packet_padding_rate != 0. {
            len += 1;
        }
        if self.bytes_padding != 0 {
            len += 1;
        }
        if self.header_bytes_padding != 0 {
            len += 1;
        }
        if self.bitrate_padding != 0. {
            len += 1;
        }
        if self.packets_out_of_order != 0 {
            len += 1;
        }
        if self.frames != 0 {
            len += 1;
        }
        if self.frame_rate != 0. {
            len += 1;
        }
        if self.jitter_current != 0. {
            len += 1;
        }
        if self.jitter_max != 0. {
            len += 1;
        }
        if !self.gap_histogram.is_empty() {
            len += 1;
        }
        if self.nacks != 0 {
            len += 1;
        }
        if self.nack_acks != 0 {
            len += 1;
        }
        if self.nack_misses != 0 {
            len += 1;
        }
        if self.nack_repeated != 0 {
            len += 1;
        }
        if self.plis != 0 {
            len += 1;
        }
        if self.last_pli.is_some() {
            len += 1;
        }
        if self.firs != 0 {
            len += 1;
        }
        if self.last_fir.is_some() {
            len += 1;
        }
        if self.rtt_current != 0 {
            len += 1;
        }
        if self.rtt_max != 0 {
            len += 1;
        }
        if self.key_frames != 0 {
            len += 1;
        }
        if self.last_key_frame.is_some() {
            len += 1;
        }
        if self.layer_lock_plis != 0 {
            len += 1;
        }
        if self.last_layer_lock_pli.is_some() {
            len += 1;
        }
        if self.packet_drift.is_some() {
            len += 1;
        }
        if self.report_drift.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.RTPStats", len)?;
        if let Some(v) = self.start_time.as_ref() {
            struct_ser.serialize_field("startTime", v)?;
        }
        if let Some(v) = self.end_time.as_ref() {
            struct_ser.serialize_field("endTime", v)?;
        }
        if self.duration != 0. {
            struct_ser.serialize_field("duration", &self.duration)?;
        }
        if self.packets != 0 {
            struct_ser.serialize_field("packets", &self.packets)?;
        }
        if self.packet_rate != 0. {
            struct_ser.serialize_field("packetRate", &self.packet_rate)?;
        }
        if self.bytes != 0 {
            struct_ser.serialize_field("bytes", ToString::to_string(&self.bytes).as_str())?;
        }
        if self.header_bytes != 0 {
            struct_ser.serialize_field("headerBytes", ToString::to_string(&self.header_bytes).as_str())?;
        }
        if self.bitrate != 0. {
            struct_ser.serialize_field("bitrate", &self.bitrate)?;
        }
        if self.packets_lost != 0 {
            struct_ser.serialize_field("packetsLost", &self.packets_lost)?;
        }
        if self.packet_loss_rate != 0. {
            struct_ser.serialize_field("packetLossRate", &self.packet_loss_rate)?;
        }
        if self.packet_loss_percentage != 0. {
            struct_ser.serialize_field("packetLossPercentage", &self.packet_loss_percentage)?;
        }
        if self.packets_duplicate != 0 {
            struct_ser.serialize_field("packetsDuplicate", &self.packets_duplicate)?;
        }
        if self.packet_duplicate_rate != 0. {
            struct_ser.serialize_field("packetDuplicateRate", &self.packet_duplicate_rate)?;
        }
        if self.bytes_duplicate != 0 {
            struct_ser.serialize_field("bytesDuplicate", ToString::to_string(&self.bytes_duplicate).as_str())?;
        }
        if self.header_bytes_duplicate != 0 {
            struct_ser.serialize_field("headerBytesDuplicate", ToString::to_string(&self.header_bytes_duplicate).as_str())?;
        }
        if self.bitrate_duplicate != 0. {
            struct_ser.serialize_field("bitrateDuplicate", &self.bitrate_duplicate)?;
        }
        if self.packets_padding != 0 {
            struct_ser.serialize_field("packetsPadding", &self.packets_padding)?;
        }
        if self.packet_padding_rate != 0. {
            struct_ser.serialize_field("packetPaddingRate", &self.packet_padding_rate)?;
        }
        if self.bytes_padding != 0 {
            struct_ser.serialize_field("bytesPadding", ToString::to_string(&self.bytes_padding).as_str())?;
        }
        if self.header_bytes_padding != 0 {
            struct_ser.serialize_field("headerBytesPadding", ToString::to_string(&self.header_bytes_padding).as_str())?;
        }
        if self.bitrate_padding != 0. {
            struct_ser.serialize_field("bitratePadding", &self.bitrate_padding)?;
        }
        if self.packets_out_of_order != 0 {
            struct_ser.serialize_field("packetsOutOfOrder", &self.packets_out_of_order)?;
        }
        if self.frames != 0 {
            struct_ser.serialize_field("frames", &self.frames)?;
        }
        if self.frame_rate != 0. {
            struct_ser.serialize_field("frameRate", &self.frame_rate)?;
        }
        if self.jitter_current != 0. {
            struct_ser.serialize_field("jitterCurrent", &self.jitter_current)?;
        }
        if self.jitter_max != 0. {
            struct_ser.serialize_field("jitterMax", &self.jitter_max)?;
        }
        if !self.gap_histogram.is_empty() {
            struct_ser.serialize_field("gapHistogram", &self.gap_histogram)?;
        }
        if self.nacks != 0 {
            struct_ser.serialize_field("nacks", &self.nacks)?;
        }
        if self.nack_acks != 0 {
            struct_ser.serialize_field("nackAcks", &self.nack_acks)?;
        }
        if self.nack_misses != 0 {
            struct_ser.serialize_field("nackMisses", &self.nack_misses)?;
        }
        if self.nack_repeated != 0 {
            struct_ser.serialize_field("nackRepeated", &self.nack_repeated)?;
        }
        if self.plis != 0 {
            struct_ser.serialize_field("plis", &self.plis)?;
        }
        if let Some(v) = self.last_pli.as_ref() {
            struct_ser.serialize_field("lastPli", v)?;
        }
        if self.firs != 0 {
            struct_ser.serialize_field("firs", &self.firs)?;
        }
        if let Some(v) = self.last_fir.as_ref() {
            struct_ser.serialize_field("lastFir", v)?;
        }
        if self.rtt_current != 0 {
            struct_ser.serialize_field("rttCurrent", &self.rtt_current)?;
        }
        if self.rtt_max != 0 {
            struct_ser.serialize_field("rttMax", &self.rtt_max)?;
        }
        if self.key_frames != 0 {
            struct_ser.serialize_field("keyFrames", &self.key_frames)?;
        }
        if let Some(v) = self.last_key_frame.as_ref() {
            struct_ser.serialize_field("lastKeyFrame", v)?;
        }
        if self.layer_lock_plis != 0 {
            struct_ser.serialize_field("layerLockPlis", &self.layer_lock_plis)?;
        }
        if let Some(v) = self.last_layer_lock_pli.as_ref() {
            struct_ser.serialize_field("lastLayerLockPli", v)?;
        }
        if let Some(v) = self.packet_drift.as_ref() {
            struct_ser.serialize_field("packetDrift", v)?;
        }
        if let Some(v) = self.report_drift.as_ref() {
            struct_ser.serialize_field("reportDrift", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RtpStats {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "start_time",
            "startTime",
            "end_time",
            "endTime",
            "duration",
            "packets",
            "packet_rate",
            "packetRate",
            "bytes",
            "header_bytes",
            "headerBytes",
            "bitrate",
            "packets_lost",
            "packetsLost",
            "packet_loss_rate",
            "packetLossRate",
            "packet_loss_percentage",
            "packetLossPercentage",
            "packets_duplicate",
            "packetsDuplicate",
            "packet_duplicate_rate",
            "packetDuplicateRate",
            "bytes_duplicate",
            "bytesDuplicate",
            "header_bytes_duplicate",
            "headerBytesDuplicate",
            "bitrate_duplicate",
            "bitrateDuplicate",
            "packets_padding",
            "packetsPadding",
            "packet_padding_rate",
            "packetPaddingRate",
            "bytes_padding",
            "bytesPadding",
            "header_bytes_padding",
            "headerBytesPadding",
            "bitrate_padding",
            "bitratePadding",
            "packets_out_of_order",
            "packetsOutOfOrder",
            "frames",
            "frame_rate",
            "frameRate",
            "jitter_current",
            "jitterCurrent",
            "jitter_max",
            "jitterMax",
            "gap_histogram",
            "gapHistogram",
            "nacks",
            "nack_acks",
            "nackAcks",
            "nack_misses",
            "nackMisses",
            "nack_repeated",
            "nackRepeated",
            "plis",
            "last_pli",
            "lastPli",
            "firs",
            "last_fir",
            "lastFir",
            "rtt_current",
            "rttCurrent",
            "rtt_max",
            "rttMax",
            "key_frames",
            "keyFrames",
            "last_key_frame",
            "lastKeyFrame",
            "layer_lock_plis",
            "layerLockPlis",
            "last_layer_lock_pli",
            "lastLayerLockPli",
            "packet_drift",
            "packetDrift",
            "report_drift",
            "reportDrift",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            StartTime,
            EndTime,
            Duration,
            Packets,
            PacketRate,
            Bytes,
            HeaderBytes,
            Bitrate,
            PacketsLost,
            PacketLossRate,
            PacketLossPercentage,
            PacketsDuplicate,
            PacketDuplicateRate,
            BytesDuplicate,
            HeaderBytesDuplicate,
            BitrateDuplicate,
            PacketsPadding,
            PacketPaddingRate,
            BytesPadding,
            HeaderBytesPadding,
            BitratePadding,
            PacketsOutOfOrder,
            Frames,
            FrameRate,
            JitterCurrent,
            JitterMax,
            GapHistogram,
            Nacks,
            NackAcks,
            NackMisses,
            NackRepeated,
            Plis,
            LastPli,
            Firs,
            LastFir,
            RttCurrent,
            RttMax,
            KeyFrames,
            LastKeyFrame,
            LayerLockPlis,
            LastLayerLockPli,
            PacketDrift,
            ReportDrift,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "startTime" | "start_time" => Ok(GeneratedField::StartTime),
                            "endTime" | "end_time" => Ok(GeneratedField::EndTime),
                            "duration" => Ok(GeneratedField::Duration),
                            "packets" => Ok(GeneratedField::Packets),
                            "packetRate" | "packet_rate" => Ok(GeneratedField::PacketRate),
                            "bytes" => Ok(GeneratedField::Bytes),
                            "headerBytes" | "header_bytes" => Ok(GeneratedField::HeaderBytes),
                            "bitrate" => Ok(GeneratedField::Bitrate),
                            "packetsLost" | "packets_lost" => Ok(GeneratedField::PacketsLost),
                            "packetLossRate" | "packet_loss_rate" => Ok(GeneratedField::PacketLossRate),
                            "packetLossPercentage" | "packet_loss_percentage" => Ok(GeneratedField::PacketLossPercentage),
                            "packetsDuplicate" | "packets_duplicate" => Ok(GeneratedField::PacketsDuplicate),
                            "packetDuplicateRate" | "packet_duplicate_rate" => Ok(GeneratedField::PacketDuplicateRate),
                            "bytesDuplicate" | "bytes_duplicate" => Ok(GeneratedField::BytesDuplicate),
                            "headerBytesDuplicate" | "header_bytes_duplicate" => Ok(GeneratedField::HeaderBytesDuplicate),
                            "bitrateDuplicate" | "bitrate_duplicate" => Ok(GeneratedField::BitrateDuplicate),
                            "packetsPadding" | "packets_padding" => Ok(GeneratedField::PacketsPadding),
                            "packetPaddingRate" | "packet_padding_rate" => Ok(GeneratedField::PacketPaddingRate),
                            "bytesPadding" | "bytes_padding" => Ok(GeneratedField::BytesPadding),
                            "headerBytesPadding" | "header_bytes_padding" => Ok(GeneratedField::HeaderBytesPadding),
                            "bitratePadding" | "bitrate_padding" => Ok(GeneratedField::BitratePadding),
                            "packetsOutOfOrder" | "packets_out_of_order" => Ok(GeneratedField::PacketsOutOfOrder),
                            "frames" => Ok(GeneratedField::Frames),
                            "frameRate" | "frame_rate" => Ok(GeneratedField::FrameRate),
                            "jitterCurrent" | "jitter_current" => Ok(GeneratedField::JitterCurrent),
                            "jitterMax" | "jitter_max" => Ok(GeneratedField::JitterMax),
                            "gapHistogram" | "gap_histogram" => Ok(GeneratedField::GapHistogram),
                            "nacks" => Ok(GeneratedField::Nacks),
                            "nackAcks" | "nack_acks" => Ok(GeneratedField::NackAcks),
                            "nackMisses" | "nack_misses" => Ok(GeneratedField::NackMisses),
                            "nackRepeated" | "nack_repeated" => Ok(GeneratedField::NackRepeated),
                            "plis" => Ok(GeneratedField::Plis),
                            "lastPli" | "last_pli" => Ok(GeneratedField::LastPli),
                            "firs" => Ok(GeneratedField::Firs),
                            "lastFir" | "last_fir" => Ok(GeneratedField::LastFir),
                            "rttCurrent" | "rtt_current" => Ok(GeneratedField::RttCurrent),
                            "rttMax" | "rtt_max" => Ok(GeneratedField::RttMax),
                            "keyFrames" | "key_frames" => Ok(GeneratedField::KeyFrames),
                            "lastKeyFrame" | "last_key_frame" => Ok(GeneratedField::LastKeyFrame),
                            "layerLockPlis" | "layer_lock_plis" => Ok(GeneratedField::LayerLockPlis),
                            "lastLayerLockPli" | "last_layer_lock_pli" => Ok(GeneratedField::LastLayerLockPli),
                            "packetDrift" | "packet_drift" => Ok(GeneratedField::PacketDrift),
                            "reportDrift" | "report_drift" => Ok(GeneratedField::ReportDrift),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RtpStats;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.RTPStats")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RtpStats, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut start_time__ = None;
                let mut end_time__ = None;
                let mut duration__ = None;
                let mut packets__ = None;
                let mut packet_rate__ = None;
                let mut bytes__ = None;
                let mut header_bytes__ = None;
                let mut bitrate__ = None;
                let mut packets_lost__ = None;
                let mut packet_loss_rate__ = None;
                let mut packet_loss_percentage__ = None;
                let mut packets_duplicate__ = None;
                let mut packet_duplicate_rate__ = None;
                let mut bytes_duplicate__ = None;
                let mut header_bytes_duplicate__ = None;
                let mut bitrate_duplicate__ = None;
                let mut packets_padding__ = None;
                let mut packet_padding_rate__ = None;
                let mut bytes_padding__ = None;
                let mut header_bytes_padding__ = None;
                let mut bitrate_padding__ = None;
                let mut packets_out_of_order__ = None;
                let mut frames__ = None;
                let mut frame_rate__ = None;
                let mut jitter_current__ = None;
                let mut jitter_max__ = None;
                let mut gap_histogram__ = None;
                let mut nacks__ = None;
                let mut nack_acks__ = None;
                let mut nack_misses__ = None;
                let mut nack_repeated__ = None;
                let mut plis__ = None;
                let mut last_pli__ = None;
                let mut firs__ = None;
                let mut last_fir__ = None;
                let mut rtt_current__ = None;
                let mut rtt_max__ = None;
                let mut key_frames__ = None;
                let mut last_key_frame__ = None;
                let mut layer_lock_plis__ = None;
                let mut last_layer_lock_pli__ = None;
                let mut packet_drift__ = None;
                let mut report_drift__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::StartTime => {
                            if start_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startTime"));
                            }
                            start_time__ = map.next_value()?;
                        }
                        GeneratedField::EndTime => {
                            if end_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endTime"));
                            }
                            end_time__ = map.next_value()?;
                        }
                        GeneratedField::Duration => {
                            if duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("duration"));
                            }
                            duration__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Packets => {
                            if packets__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packets"));
                            }
                            packets__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PacketRate => {
                            if packet_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetRate"));
                            }
                            packet_rate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Bytes => {
                            if bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bytes"));
                            }
                            bytes__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::HeaderBytes => {
                            if header_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("headerBytes"));
                            }
                            header_bytes__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Bitrate => {
                            if bitrate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bitrate"));
                            }
                            bitrate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PacketsLost => {
                            if packets_lost__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetsLost"));
                            }
                            packets_lost__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PacketLossRate => {
                            if packet_loss_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetLossRate"));
                            }
                            packet_loss_rate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PacketLossPercentage => {
                            if packet_loss_percentage__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetLossPercentage"));
                            }
                            packet_loss_percentage__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PacketsDuplicate => {
                            if packets_duplicate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetsDuplicate"));
                            }
                            packets_duplicate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PacketDuplicateRate => {
                            if packet_duplicate_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetDuplicateRate"));
                            }
                            packet_duplicate_rate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::BytesDuplicate => {
                            if bytes_duplicate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bytesDuplicate"));
                            }
                            bytes_duplicate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::HeaderBytesDuplicate => {
                            if header_bytes_duplicate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("headerBytesDuplicate"));
                            }
                            header_bytes_duplicate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::BitrateDuplicate => {
                            if bitrate_duplicate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bitrateDuplicate"));
                            }
                            bitrate_duplicate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PacketsPadding => {
                            if packets_padding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetsPadding"));
                            }
                            packets_padding__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PacketPaddingRate => {
                            if packet_padding_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetPaddingRate"));
                            }
                            packet_padding_rate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::BytesPadding => {
                            if bytes_padding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bytesPadding"));
                            }
                            bytes_padding__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::HeaderBytesPadding => {
                            if header_bytes_padding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("headerBytesPadding"));
                            }
                            header_bytes_padding__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::BitratePadding => {
                            if bitrate_padding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bitratePadding"));
                            }
                            bitrate_padding__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PacketsOutOfOrder => {
                            if packets_out_of_order__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetsOutOfOrder"));
                            }
                            packets_out_of_order__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Frames => {
                            if frames__.is_some() {
                                return Err(serde::de::Error::duplicate_field("frames"));
                            }
                            frames__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::FrameRate => {
                            if frame_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("frameRate"));
                            }
                            frame_rate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::JitterCurrent => {
                            if jitter_current__.is_some() {
                                return Err(serde::de::Error::duplicate_field("jitterCurrent"));
                            }
                            jitter_current__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::JitterMax => {
                            if jitter_max__.is_some() {
                                return Err(serde::de::Error::duplicate_field("jitterMax"));
                            }
                            jitter_max__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GapHistogram => {
                            if gap_histogram__.is_some() {
                                return Err(serde::de::Error::duplicate_field("gapHistogram"));
                            }
                            gap_histogram__ = Some(
                                map.next_value::<std::collections::HashMap<::pbjson::private::NumberDeserialize<i32>, ::pbjson::private::NumberDeserialize<u32>>>()?
                                    .into_iter().map(|(k,v)| (k.0, v.0)).collect()
                            );
                        }
                        GeneratedField::Nacks => {
                            if nacks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nacks"));
                            }
                            nacks__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NackAcks => {
                            if nack_acks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nackAcks"));
                            }
                            nack_acks__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NackMisses => {
                            if nack_misses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nackMisses"));
                            }
                            nack_misses__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NackRepeated => {
                            if nack_repeated__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nackRepeated"));
                            }
                            nack_repeated__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Plis => {
                            if plis__.is_some() {
                                return Err(serde::de::Error::duplicate_field("plis"));
                            }
                            plis__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::LastPli => {
                            if last_pli__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastPli"));
                            }
                            last_pli__ = map.next_value()?;
                        }
                        GeneratedField::Firs => {
                            if firs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("firs"));
                            }
                            firs__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::LastFir => {
                            if last_fir__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastFir"));
                            }
                            last_fir__ = map.next_value()?;
                        }
                        GeneratedField::RttCurrent => {
                            if rtt_current__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rttCurrent"));
                            }
                            rtt_current__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RttMax => {
                            if rtt_max__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rttMax"));
                            }
                            rtt_max__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::KeyFrames => {
                            if key_frames__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyFrames"));
                            }
                            key_frames__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::LastKeyFrame => {
                            if last_key_frame__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastKeyFrame"));
                            }
                            last_key_frame__ = map.next_value()?;
                        }
                        GeneratedField::LayerLockPlis => {
                            if layer_lock_plis__.is_some() {
                                return Err(serde::de::Error::duplicate_field("layerLockPlis"));
                            }
                            layer_lock_plis__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::LastLayerLockPli => {
                            if last_layer_lock_pli__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastLayerLockPli"));
                            }
                            last_layer_lock_pli__ = map.next_value()?;
                        }
                        GeneratedField::PacketDrift => {
                            if packet_drift__.is_some() {
                                return Err(serde::de::Error::duplicate_field("packetDrift"));
                            }
                            packet_drift__ = map.next_value()?;
                        }
                        GeneratedField::ReportDrift => {
                            if report_drift__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reportDrift"));
                            }
                            report_drift__ = map.next_value()?;
                        }
                    }
                }
                Ok(RtpStats {
                    start_time: start_time__,
                    end_time: end_time__,
                    duration: duration__.unwrap_or_default(),
                    packets: packets__.unwrap_or_default(),
                    packet_rate: packet_rate__.unwrap_or_default(),
                    bytes: bytes__.unwrap_or_default(),
                    header_bytes: header_bytes__.unwrap_or_default(),
                    bitrate: bitrate__.unwrap_or_default(),
                    packets_lost: packets_lost__.unwrap_or_default(),
                    packet_loss_rate: packet_loss_rate__.unwrap_or_default(),
                    packet_loss_percentage: packet_loss_percentage__.unwrap_or_default(),
                    packets_duplicate: packets_duplicate__.unwrap_or_default(),
                    packet_duplicate_rate: packet_duplicate_rate__.unwrap_or_default(),
                    bytes_duplicate: bytes_duplicate__.unwrap_or_default(),
                    header_bytes_duplicate: header_bytes_duplicate__.unwrap_or_default(),
                    bitrate_duplicate: bitrate_duplicate__.unwrap_or_default(),
                    packets_padding: packets_padding__.unwrap_or_default(),
                    packet_padding_rate: packet_padding_rate__.unwrap_or_default(),
                    bytes_padding: bytes_padding__.unwrap_or_default(),
                    header_bytes_padding: header_bytes_padding__.unwrap_or_default(),
                    bitrate_padding: bitrate_padding__.unwrap_or_default(),
                    packets_out_of_order: packets_out_of_order__.unwrap_or_default(),
                    frames: frames__.unwrap_or_default(),
                    frame_rate: frame_rate__.unwrap_or_default(),
                    jitter_current: jitter_current__.unwrap_or_default(),
                    jitter_max: jitter_max__.unwrap_or_default(),
                    gap_histogram: gap_histogram__.unwrap_or_default(),
                    nacks: nacks__.unwrap_or_default(),
                    nack_acks: nack_acks__.unwrap_or_default(),
                    nack_misses: nack_misses__.unwrap_or_default(),
                    nack_repeated: nack_repeated__.unwrap_or_default(),
                    plis: plis__.unwrap_or_default(),
                    last_pli: last_pli__,
                    firs: firs__.unwrap_or_default(),
                    last_fir: last_fir__,
                    rtt_current: rtt_current__.unwrap_or_default(),
                    rtt_max: rtt_max__.unwrap_or_default(),
                    key_frames: key_frames__.unwrap_or_default(),
                    last_key_frame: last_key_frame__,
                    layer_lock_plis: layer_lock_plis__.unwrap_or_default(),
                    last_layer_lock_pli: last_layer_lock_pli__,
                    packet_drift: packet_drift__,
                    report_drift: report_drift__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.RTPStats", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ReconnectReason {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::RrUnknown => "RR_UNKNOWN",
            Self::RrSignalDisconnected => "RR_SIGNAL_DISCONNECTED",
            Self::RrPublisherFailed => "RR_PUBLISHER_FAILED",
            Self::RrSubscriberFailed => "RR_SUBSCRIBER_FAILED",
            Self::RrSwitchCandidate => "RR_SWITCH_CANDIDATE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ReconnectReason {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "RR_UNKNOWN",
            "RR_SIGNAL_DISCONNECTED",
            "RR_PUBLISHER_FAILED",
            "RR_SUBSCRIBER_FAILED",
            "RR_SWITCH_CANDIDATE",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ReconnectReason;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ReconnectReason::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(ReconnectReason::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "RR_UNKNOWN" => Ok(ReconnectReason::RrUnknown),
                    "RR_SIGNAL_DISCONNECTED" => Ok(ReconnectReason::RrSignalDisconnected),
                    "RR_PUBLISHER_FAILED" => Ok(ReconnectReason::RrPublisherFailed),
                    "RR_SUBSCRIBER_FAILED" => Ok(ReconnectReason::RrSubscriberFailed),
                    "RR_SWITCH_CANDIDATE" => Ok(ReconnectReason::RrSwitchCandidate),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ReconnectResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.ice_servers.is_empty() {
            len += 1;
        }
        if self.client_configuration.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ReconnectResponse", len)?;
        if !self.ice_servers.is_empty() {
            struct_ser.serialize_field("iceServers", &self.ice_servers)?;
        }
        if let Some(v) = self.client_configuration.as_ref() {
            struct_ser.serialize_field("clientConfiguration", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ReconnectResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ice_servers",
            "iceServers",
            "client_configuration",
            "clientConfiguration",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IceServers,
            ClientConfiguration,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "iceServers" | "ice_servers" => Ok(GeneratedField::IceServers),
                            "clientConfiguration" | "client_configuration" => Ok(GeneratedField::ClientConfiguration),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ReconnectResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ReconnectResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ReconnectResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut ice_servers__ = None;
                let mut client_configuration__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IceServers => {
                            if ice_servers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("iceServers"));
                            }
                            ice_servers__ = Some(map.next_value()?);
                        }
                        GeneratedField::ClientConfiguration => {
                            if client_configuration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("clientConfiguration"));
                            }
                            client_configuration__ = map.next_value()?;
                        }
                    }
                }
                Ok(ReconnectResponse {
                    ice_servers: ice_servers__.unwrap_or_default(),
                    client_configuration: client_configuration__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.ReconnectResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RegionInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.region.is_empty() {
            len += 1;
        }
        if !self.url.is_empty() {
            len += 1;
        }
        if self.distance != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.RegionInfo", len)?;
        if !self.region.is_empty() {
            struct_ser.serialize_field("region", &self.region)?;
        }
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        if self.distance != 0 {
            struct_ser.serialize_field("distance", ToString::to_string(&self.distance).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RegionInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "region",
            "url",
            "distance",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Region,
            Url,
            Distance,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "region" => Ok(GeneratedField::Region),
                            "url" => Ok(GeneratedField::Url),
                            "distance" => Ok(GeneratedField::Distance),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RegionInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.RegionInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RegionInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut region__ = None;
                let mut url__ = None;
                let mut distance__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Region => {
                            if region__.is_some() {
                                return Err(serde::de::Error::duplicate_field("region"));
                            }
                            region__ = Some(map.next_value()?);
                        }
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map.next_value()?);
                        }
                        GeneratedField::Distance => {
                            if distance__.is_some() {
                                return Err(serde::de::Error::duplicate_field("distance"));
                            }
                            distance__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(RegionInfo {
                    region: region__.unwrap_or_default(),
                    url: url__.unwrap_or_default(),
                    distance: distance__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.RegionInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RegionSettings {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.regions.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.RegionSettings", len)?;
        if !self.regions.is_empty() {
            struct_ser.serialize_field("regions", &self.regions)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RegionSettings {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "regions",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Regions,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "regions" => Ok(GeneratedField::Regions),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RegionSettings;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.RegionSettings")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RegionSettings, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut regions__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Regions => {
                            if regions__.is_some() {
                                return Err(serde::de::Error::duplicate_field("regions"));
                            }
                            regions__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(RegionSettings {
                    regions: regions__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.RegionSettings", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RemoveParticipantResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.RemoveParticipantResponse", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RemoveParticipantResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Err(serde::de::Error::unknown_field(value, FIELDS))
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RemoveParticipantResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.RemoveParticipantResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RemoveParticipantResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map.next_key::<GeneratedField>()?.is_some() {
                    let _ = map.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(RemoveParticipantResponse {
                })
            }
        }
        deserializer.deserialize_struct("livekit.RemoveParticipantResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Room {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.sid.is_empty() {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        if self.empty_timeout != 0 {
            len += 1;
        }
        if self.max_participants != 0 {
            len += 1;
        }
        if self.creation_time != 0 {
            len += 1;
        }
        if !self.turn_password.is_empty() {
            len += 1;
        }
        if !self.enabled_codecs.is_empty() {
            len += 1;
        }
        if !self.metadata.is_empty() {
            len += 1;
        }
        if self.num_participants != 0 {
            len += 1;
        }
        if self.num_publishers != 0 {
            len += 1;
        }
        if self.active_recording {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.Room", len)?;
        if !self.sid.is_empty() {
            struct_ser.serialize_field("sid", &self.sid)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if self.empty_timeout != 0 {
            struct_ser.serialize_field("emptyTimeout", &self.empty_timeout)?;
        }
        if self.max_participants != 0 {
            struct_ser.serialize_field("maxParticipants", &self.max_participants)?;
        }
        if self.creation_time != 0 {
            struct_ser.serialize_field("creationTime", ToString::to_string(&self.creation_time).as_str())?;
        }
        if !self.turn_password.is_empty() {
            struct_ser.serialize_field("turnPassword", &self.turn_password)?;
        }
        if !self.enabled_codecs.is_empty() {
            struct_ser.serialize_field("enabledCodecs", &self.enabled_codecs)?;
        }
        if !self.metadata.is_empty() {
            struct_ser.serialize_field("metadata", &self.metadata)?;
        }
        if self.num_participants != 0 {
            struct_ser.serialize_field("numParticipants", &self.num_participants)?;
        }
        if self.num_publishers != 0 {
            struct_ser.serialize_field("numPublishers", &self.num_publishers)?;
        }
        if self.active_recording {
            struct_ser.serialize_field("activeRecording", &self.active_recording)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Room {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sid",
            "name",
            "empty_timeout",
            "emptyTimeout",
            "max_participants",
            "maxParticipants",
            "creation_time",
            "creationTime",
            "turn_password",
            "turnPassword",
            "enabled_codecs",
            "enabledCodecs",
            "metadata",
            "num_participants",
            "numParticipants",
            "num_publishers",
            "numPublishers",
            "active_recording",
            "activeRecording",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Sid,
            Name,
            EmptyTimeout,
            MaxParticipants,
            CreationTime,
            TurnPassword,
            EnabledCodecs,
            Metadata,
            NumParticipants,
            NumPublishers,
            ActiveRecording,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "sid" => Ok(GeneratedField::Sid),
                            "name" => Ok(GeneratedField::Name),
                            "emptyTimeout" | "empty_timeout" => Ok(GeneratedField::EmptyTimeout),
                            "maxParticipants" | "max_participants" => Ok(GeneratedField::MaxParticipants),
                            "creationTime" | "creation_time" => Ok(GeneratedField::CreationTime),
                            "turnPassword" | "turn_password" => Ok(GeneratedField::TurnPassword),
                            "enabledCodecs" | "enabled_codecs" => Ok(GeneratedField::EnabledCodecs),
                            "metadata" => Ok(GeneratedField::Metadata),
                            "numParticipants" | "num_participants" => Ok(GeneratedField::NumParticipants),
                            "numPublishers" | "num_publishers" => Ok(GeneratedField::NumPublishers),
                            "activeRecording" | "active_recording" => Ok(GeneratedField::ActiveRecording),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Room;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.Room")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<Room, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sid__ = None;
                let mut name__ = None;
                let mut empty_timeout__ = None;
                let mut max_participants__ = None;
                let mut creation_time__ = None;
                let mut turn_password__ = None;
                let mut enabled_codecs__ = None;
                let mut metadata__ = None;
                let mut num_participants__ = None;
                let mut num_publishers__ = None;
                let mut active_recording__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Sid => {
                            if sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sid"));
                            }
                            sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::EmptyTimeout => {
                            if empty_timeout__.is_some() {
                                return Err(serde::de::Error::duplicate_field("emptyTimeout"));
                            }
                            empty_timeout__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MaxParticipants => {
                            if max_participants__.is_some() {
                                return Err(serde::de::Error::duplicate_field("maxParticipants"));
                            }
                            max_participants__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CreationTime => {
                            if creation_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("creationTime"));
                            }
                            creation_time__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TurnPassword => {
                            if turn_password__.is_some() {
                                return Err(serde::de::Error::duplicate_field("turnPassword"));
                            }
                            turn_password__ = Some(map.next_value()?);
                        }
                        GeneratedField::EnabledCodecs => {
                            if enabled_codecs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("enabledCodecs"));
                            }
                            enabled_codecs__ = Some(map.next_value()?);
                        }
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = Some(map.next_value()?);
                        }
                        GeneratedField::NumParticipants => {
                            if num_participants__.is_some() {
                                return Err(serde::de::Error::duplicate_field("numParticipants"));
                            }
                            num_participants__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NumPublishers => {
                            if num_publishers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("numPublishers"));
                            }
                            num_publishers__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ActiveRecording => {
                            if active_recording__.is_some() {
                                return Err(serde::de::Error::duplicate_field("activeRecording"));
                            }
                            active_recording__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(Room {
                    sid: sid__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                    empty_timeout: empty_timeout__.unwrap_or_default(),
                    max_participants: max_participants__.unwrap_or_default(),
                    creation_time: creation_time__.unwrap_or_default(),
                    turn_password: turn_password__.unwrap_or_default(),
                    enabled_codecs: enabled_codecs__.unwrap_or_default(),
                    metadata: metadata__.unwrap_or_default(),
                    num_participants: num_participants__.unwrap_or_default(),
                    num_publishers: num_publishers__.unwrap_or_default(),
                    active_recording: active_recording__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.Room", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoomCompositeEgressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room_name.is_empty() {
            len += 1;
        }
        if !self.layout.is_empty() {
            len += 1;
        }
        if self.audio_only {
            len += 1;
        }
        if self.video_only {
            len += 1;
        }
        if !self.custom_base_url.is_empty() {
            len += 1;
        }
        if !self.file_outputs.is_empty() {
            len += 1;
        }
        if !self.stream_outputs.is_empty() {
            len += 1;
        }
        if !self.segment_outputs.is_empty() {
            len += 1;
        }
        if !self.image_outputs.is_empty() {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        if self.options.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.RoomCompositeEgressRequest", len)?;
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if !self.layout.is_empty() {
            struct_ser.serialize_field("layout", &self.layout)?;
        }
        if self.audio_only {
            struct_ser.serialize_field("audioOnly", &self.audio_only)?;
        }
        if self.video_only {
            struct_ser.serialize_field("videoOnly", &self.video_only)?;
        }
        if !self.custom_base_url.is_empty() {
            struct_ser.serialize_field("customBaseUrl", &self.custom_base_url)?;
        }
        if !self.file_outputs.is_empty() {
            struct_ser.serialize_field("fileOutputs", &self.file_outputs)?;
        }
        if !self.stream_outputs.is_empty() {
            struct_ser.serialize_field("streamOutputs", &self.stream_outputs)?;
        }
        if !self.segment_outputs.is_empty() {
            struct_ser.serialize_field("segmentOutputs", &self.segment_outputs)?;
        }
        if !self.image_outputs.is_empty() {
            struct_ser.serialize_field("imageOutputs", &self.image_outputs)?;
        }
        if let Some(v) = self.output.as_ref() {
            match v {
                room_composite_egress_request::Output::File(v) => {
                    struct_ser.serialize_field("file", v)?;
                }
                room_composite_egress_request::Output::Stream(v) => {
                    struct_ser.serialize_field("stream", v)?;
                }
                room_composite_egress_request::Output::Segments(v) => {
                    struct_ser.serialize_field("segments", v)?;
                }
            }
        }
        if let Some(v) = self.options.as_ref() {
            match v {
                room_composite_egress_request::Options::Preset(v) => {
                    let v = EncodingOptionsPreset::from_i32(*v)
                        .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("preset", &v)?;
                }
                room_composite_egress_request::Options::Advanced(v) => {
                    struct_ser.serialize_field("advanced", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoomCompositeEgressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room_name",
            "roomName",
            "layout",
            "audio_only",
            "audioOnly",
            "video_only",
            "videoOnly",
            "custom_base_url",
            "customBaseUrl",
            "file_outputs",
            "fileOutputs",
            "stream_outputs",
            "streamOutputs",
            "segment_outputs",
            "segmentOutputs",
            "image_outputs",
            "imageOutputs",
            "file",
            "stream",
            "segments",
            "preset",
            "advanced",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RoomName,
            Layout,
            AudioOnly,
            VideoOnly,
            CustomBaseUrl,
            FileOutputs,
            StreamOutputs,
            SegmentOutputs,
            ImageOutputs,
            File,
            Stream,
            Segments,
            Preset,
            Advanced,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "layout" => Ok(GeneratedField::Layout),
                            "audioOnly" | "audio_only" => Ok(GeneratedField::AudioOnly),
                            "videoOnly" | "video_only" => Ok(GeneratedField::VideoOnly),
                            "customBaseUrl" | "custom_base_url" => Ok(GeneratedField::CustomBaseUrl),
                            "fileOutputs" | "file_outputs" => Ok(GeneratedField::FileOutputs),
                            "streamOutputs" | "stream_outputs" => Ok(GeneratedField::StreamOutputs),
                            "segmentOutputs" | "segment_outputs" => Ok(GeneratedField::SegmentOutputs),
                            "imageOutputs" | "image_outputs" => Ok(GeneratedField::ImageOutputs),
                            "file" => Ok(GeneratedField::File),
                            "stream" => Ok(GeneratedField::Stream),
                            "segments" => Ok(GeneratedField::Segments),
                            "preset" => Ok(GeneratedField::Preset),
                            "advanced" => Ok(GeneratedField::Advanced),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoomCompositeEgressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.RoomCompositeEgressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RoomCompositeEgressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room_name__ = None;
                let mut layout__ = None;
                let mut audio_only__ = None;
                let mut video_only__ = None;
                let mut custom_base_url__ = None;
                let mut file_outputs__ = None;
                let mut stream_outputs__ = None;
                let mut segment_outputs__ = None;
                let mut image_outputs__ = None;
                let mut output__ = None;
                let mut options__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Layout => {
                            if layout__.is_some() {
                                return Err(serde::de::Error::duplicate_field("layout"));
                            }
                            layout__ = Some(map.next_value()?);
                        }
                        GeneratedField::AudioOnly => {
                            if audio_only__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioOnly"));
                            }
                            audio_only__ = Some(map.next_value()?);
                        }
                        GeneratedField::VideoOnly => {
                            if video_only__.is_some() {
                                return Err(serde::de::Error::duplicate_field("videoOnly"));
                            }
                            video_only__ = Some(map.next_value()?);
                        }
                        GeneratedField::CustomBaseUrl => {
                            if custom_base_url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("customBaseUrl"));
                            }
                            custom_base_url__ = Some(map.next_value()?);
                        }
                        GeneratedField::FileOutputs => {
                            if file_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fileOutputs"));
                            }
                            file_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::StreamOutputs => {
                            if stream_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("streamOutputs"));
                            }
                            stream_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::SegmentOutputs => {
                            if segment_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segmentOutputs"));
                            }
                            segment_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::ImageOutputs => {
                            if image_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("imageOutputs"));
                            }
                            image_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::File => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("file"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(room_composite_egress_request::Output::File)
;
                        }
                        GeneratedField::Stream => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stream"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(room_composite_egress_request::Output::Stream)
;
                        }
                        GeneratedField::Segments => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segments"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(room_composite_egress_request::Output::Segments)
;
                        }
                        GeneratedField::Preset => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preset"));
                            }
                            options__ = map.next_value::<::std::option::Option<EncodingOptionsPreset>>()?.map(|x| room_composite_egress_request::Options::Preset(x as i32));
                        }
                        GeneratedField::Advanced => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("advanced"));
                            }
                            options__ = map.next_value::<::std::option::Option<_>>()?.map(room_composite_egress_request::Options::Advanced)
;
                        }
                    }
                }
                Ok(RoomCompositeEgressRequest {
                    room_name: room_name__.unwrap_or_default(),
                    layout: layout__.unwrap_or_default(),
                    audio_only: audio_only__.unwrap_or_default(),
                    video_only: video_only__.unwrap_or_default(),
                    custom_base_url: custom_base_url__.unwrap_or_default(),
                    file_outputs: file_outputs__.unwrap_or_default(),
                    stream_outputs: stream_outputs__.unwrap_or_default(),
                    segment_outputs: segment_outputs__.unwrap_or_default(),
                    image_outputs: image_outputs__.unwrap_or_default(),
                    output: output__,
                    options: options__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.RoomCompositeEgressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoomEgress {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.room.is_some() {
            len += 1;
        }
        if self.participant.is_some() {
            len += 1;
        }
        if self.tracks.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.RoomEgress", len)?;
        if let Some(v) = self.room.as_ref() {
            struct_ser.serialize_field("room", v)?;
        }
        if let Some(v) = self.participant.as_ref() {
            struct_ser.serialize_field("participant", v)?;
        }
        if let Some(v) = self.tracks.as_ref() {
            struct_ser.serialize_field("tracks", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoomEgress {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
            "participant",
            "tracks",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
            Participant,
            Tracks,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            "participant" => Ok(GeneratedField::Participant),
                            "tracks" => Ok(GeneratedField::Tracks),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoomEgress;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.RoomEgress")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RoomEgress, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                let mut participant__ = None;
                let mut tracks__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = map.next_value()?;
                        }
                        GeneratedField::Participant => {
                            if participant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participant"));
                            }
                            participant__ = map.next_value()?;
                        }
                        GeneratedField::Tracks => {
                            if tracks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tracks"));
                            }
                            tracks__ = map.next_value()?;
                        }
                    }
                }
                Ok(RoomEgress {
                    room: room__,
                    participant: participant__,
                    tracks: tracks__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.RoomEgress", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoomParticipantIdentity {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room.is_empty() {
            len += 1;
        }
        if !self.identity.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.RoomParticipantIdentity", len)?;
        if !self.room.is_empty() {
            struct_ser.serialize_field("room", &self.room)?;
        }
        if !self.identity.is_empty() {
            struct_ser.serialize_field("identity", &self.identity)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoomParticipantIdentity {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
            "identity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
            Identity,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            "identity" => Ok(GeneratedField::Identity),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoomParticipantIdentity;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.RoomParticipantIdentity")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RoomParticipantIdentity, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                let mut identity__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = Some(map.next_value()?);
                        }
                        GeneratedField::Identity => {
                            if identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identity"));
                            }
                            identity__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(RoomParticipantIdentity {
                    room: room__.unwrap_or_default(),
                    identity: identity__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.RoomParticipantIdentity", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RoomUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.room.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.RoomUpdate", len)?;
        if let Some(v) = self.room.as_ref() {
            struct_ser.serialize_field("room", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RoomUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RoomUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.RoomUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RoomUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = map.next_value()?;
                        }
                    }
                }
                Ok(RoomUpdate {
                    room: room__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.RoomUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for S3Upload {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.access_key.is_empty() {
            len += 1;
        }
        if !self.secret.is_empty() {
            len += 1;
        }
        if !self.region.is_empty() {
            len += 1;
        }
        if !self.endpoint.is_empty() {
            len += 1;
        }
        if !self.bucket.is_empty() {
            len += 1;
        }
        if self.force_path_style {
            len += 1;
        }
        if !self.metadata.is_empty() {
            len += 1;
        }
        if !self.tagging.is_empty() {
            len += 1;
        }
        if !self.content_disposition.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.S3Upload", len)?;
        if !self.access_key.is_empty() {
            struct_ser.serialize_field("accessKey", &self.access_key)?;
        }
        if !self.secret.is_empty() {
            struct_ser.serialize_field("secret", &self.secret)?;
        }
        if !self.region.is_empty() {
            struct_ser.serialize_field("region", &self.region)?;
        }
        if !self.endpoint.is_empty() {
            struct_ser.serialize_field("endpoint", &self.endpoint)?;
        }
        if !self.bucket.is_empty() {
            struct_ser.serialize_field("bucket", &self.bucket)?;
        }
        if self.force_path_style {
            struct_ser.serialize_field("forcePathStyle", &self.force_path_style)?;
        }
        if !self.metadata.is_empty() {
            struct_ser.serialize_field("metadata", &self.metadata)?;
        }
        if !self.tagging.is_empty() {
            struct_ser.serialize_field("tagging", &self.tagging)?;
        }
        if !self.content_disposition.is_empty() {
            struct_ser.serialize_field("contentDisposition", &self.content_disposition)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for S3Upload {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "access_key",
            "accessKey",
            "secret",
            "region",
            "endpoint",
            "bucket",
            "force_path_style",
            "forcePathStyle",
            "metadata",
            "tagging",
            "content_disposition",
            "contentDisposition",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AccessKey,
            Secret,
            Region,
            Endpoint,
            Bucket,
            ForcePathStyle,
            Metadata,
            Tagging,
            ContentDisposition,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "accessKey" | "access_key" => Ok(GeneratedField::AccessKey),
                            "secret" => Ok(GeneratedField::Secret),
                            "region" => Ok(GeneratedField::Region),
                            "endpoint" => Ok(GeneratedField::Endpoint),
                            "bucket" => Ok(GeneratedField::Bucket),
                            "forcePathStyle" | "force_path_style" => Ok(GeneratedField::ForcePathStyle),
                            "metadata" => Ok(GeneratedField::Metadata),
                            "tagging" => Ok(GeneratedField::Tagging),
                            "contentDisposition" | "content_disposition" => Ok(GeneratedField::ContentDisposition),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = S3Upload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.S3Upload")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<S3Upload, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut access_key__ = None;
                let mut secret__ = None;
                let mut region__ = None;
                let mut endpoint__ = None;
                let mut bucket__ = None;
                let mut force_path_style__ = None;
                let mut metadata__ = None;
                let mut tagging__ = None;
                let mut content_disposition__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::AccessKey => {
                            if access_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accessKey"));
                            }
                            access_key__ = Some(map.next_value()?);
                        }
                        GeneratedField::Secret => {
                            if secret__.is_some() {
                                return Err(serde::de::Error::duplicate_field("secret"));
                            }
                            secret__ = Some(map.next_value()?);
                        }
                        GeneratedField::Region => {
                            if region__.is_some() {
                                return Err(serde::de::Error::duplicate_field("region"));
                            }
                            region__ = Some(map.next_value()?);
                        }
                        GeneratedField::Endpoint => {
                            if endpoint__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endpoint"));
                            }
                            endpoint__ = Some(map.next_value()?);
                        }
                        GeneratedField::Bucket => {
                            if bucket__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bucket"));
                            }
                            bucket__ = Some(map.next_value()?);
                        }
                        GeneratedField::ForcePathStyle => {
                            if force_path_style__.is_some() {
                                return Err(serde::de::Error::duplicate_field("forcePathStyle"));
                            }
                            force_path_style__ = Some(map.next_value()?);
                        }
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = Some(
                                map.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                        GeneratedField::Tagging => {
                            if tagging__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tagging"));
                            }
                            tagging__ = Some(map.next_value()?);
                        }
                        GeneratedField::ContentDisposition => {
                            if content_disposition__.is_some() {
                                return Err(serde::de::Error::duplicate_field("contentDisposition"));
                            }
                            content_disposition__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(S3Upload {
                    access_key: access_key__.unwrap_or_default(),
                    secret: secret__.unwrap_or_default(),
                    region: region__.unwrap_or_default(),
                    endpoint: endpoint__.unwrap_or_default(),
                    bucket: bucket__.unwrap_or_default(),
                    force_path_style: force_path_style__.unwrap_or_default(),
                    metadata: metadata__.unwrap_or_default(),
                    tagging: tagging__.unwrap_or_default(),
                    content_disposition: content_disposition__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.S3Upload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SegmentedFileOutput {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.protocol != 0 {
            len += 1;
        }
        if !self.filename_prefix.is_empty() {
            len += 1;
        }
        if !self.playlist_name.is_empty() {
            len += 1;
        }
        if !self.live_playlist_name.is_empty() {
            len += 1;
        }
        if self.segment_duration != 0 {
            len += 1;
        }
        if self.filename_suffix != 0 {
            len += 1;
        }
        if self.disable_manifest {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SegmentedFileOutput", len)?;
        if self.protocol != 0 {
            let v = SegmentedFileProtocol::from_i32(self.protocol)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.protocol)))?;
            struct_ser.serialize_field("protocol", &v)?;
        }
        if !self.filename_prefix.is_empty() {
            struct_ser.serialize_field("filenamePrefix", &self.filename_prefix)?;
        }
        if !self.playlist_name.is_empty() {
            struct_ser.serialize_field("playlistName", &self.playlist_name)?;
        }
        if !self.live_playlist_name.is_empty() {
            struct_ser.serialize_field("livePlaylistName", &self.live_playlist_name)?;
        }
        if self.segment_duration != 0 {
            struct_ser.serialize_field("segmentDuration", &self.segment_duration)?;
        }
        if self.filename_suffix != 0 {
            let v = SegmentedFileSuffix::from_i32(self.filename_suffix)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.filename_suffix)))?;
            struct_ser.serialize_field("filenameSuffix", &v)?;
        }
        if self.disable_manifest {
            struct_ser.serialize_field("disableManifest", &self.disable_manifest)?;
        }
        if let Some(v) = self.output.as_ref() {
            match v {
                segmented_file_output::Output::S3(v) => {
                    struct_ser.serialize_field("s3", v)?;
                }
                segmented_file_output::Output::Gcp(v) => {
                    struct_ser.serialize_field("gcp", v)?;
                }
                segmented_file_output::Output::Azure(v) => {
                    struct_ser.serialize_field("azure", v)?;
                }
                segmented_file_output::Output::AliOss(v) => {
                    struct_ser.serialize_field("aliOSS", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SegmentedFileOutput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "protocol",
            "filename_prefix",
            "filenamePrefix",
            "playlist_name",
            "playlistName",
            "live_playlist_name",
            "livePlaylistName",
            "segment_duration",
            "segmentDuration",
            "filename_suffix",
            "filenameSuffix",
            "disable_manifest",
            "disableManifest",
            "s3",
            "gcp",
            "azure",
            "aliOSS",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Protocol,
            FilenamePrefix,
            PlaylistName,
            LivePlaylistName,
            SegmentDuration,
            FilenameSuffix,
            DisableManifest,
            S3,
            Gcp,
            Azure,
            AliOss,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "protocol" => Ok(GeneratedField::Protocol),
                            "filenamePrefix" | "filename_prefix" => Ok(GeneratedField::FilenamePrefix),
                            "playlistName" | "playlist_name" => Ok(GeneratedField::PlaylistName),
                            "livePlaylistName" | "live_playlist_name" => Ok(GeneratedField::LivePlaylistName),
                            "segmentDuration" | "segment_duration" => Ok(GeneratedField::SegmentDuration),
                            "filenameSuffix" | "filename_suffix" => Ok(GeneratedField::FilenameSuffix),
                            "disableManifest" | "disable_manifest" => Ok(GeneratedField::DisableManifest),
                            "s3" => Ok(GeneratedField::S3),
                            "gcp" => Ok(GeneratedField::Gcp),
                            "azure" => Ok(GeneratedField::Azure),
                            "aliOSS" => Ok(GeneratedField::AliOss),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SegmentedFileOutput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SegmentedFileOutput")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SegmentedFileOutput, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut protocol__ = None;
                let mut filename_prefix__ = None;
                let mut playlist_name__ = None;
                let mut live_playlist_name__ = None;
                let mut segment_duration__ = None;
                let mut filename_suffix__ = None;
                let mut disable_manifest__ = None;
                let mut output__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Protocol => {
                            if protocol__.is_some() {
                                return Err(serde::de::Error::duplicate_field("protocol"));
                            }
                            protocol__ = Some(map.next_value::<SegmentedFileProtocol>()? as i32);
                        }
                        GeneratedField::FilenamePrefix => {
                            if filename_prefix__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filenamePrefix"));
                            }
                            filename_prefix__ = Some(map.next_value()?);
                        }
                        GeneratedField::PlaylistName => {
                            if playlist_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("playlistName"));
                            }
                            playlist_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::LivePlaylistName => {
                            if live_playlist_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("livePlaylistName"));
                            }
                            live_playlist_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::SegmentDuration => {
                            if segment_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segmentDuration"));
                            }
                            segment_duration__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::FilenameSuffix => {
                            if filename_suffix__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filenameSuffix"));
                            }
                            filename_suffix__ = Some(map.next_value::<SegmentedFileSuffix>()? as i32);
                        }
                        GeneratedField::DisableManifest => {
                            if disable_manifest__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableManifest"));
                            }
                            disable_manifest__ = Some(map.next_value()?);
                        }
                        GeneratedField::S3 => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("s3"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(segmented_file_output::Output::S3)
;
                        }
                        GeneratedField::Gcp => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("gcp"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(segmented_file_output::Output::Gcp)
;
                        }
                        GeneratedField::Azure => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("azure"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(segmented_file_output::Output::Azure)
;
                        }
                        GeneratedField::AliOss => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("aliOSS"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(segmented_file_output::Output::AliOss)
;
                        }
                    }
                }
                Ok(SegmentedFileOutput {
                    protocol: protocol__.unwrap_or_default(),
                    filename_prefix: filename_prefix__.unwrap_or_default(),
                    playlist_name: playlist_name__.unwrap_or_default(),
                    live_playlist_name: live_playlist_name__.unwrap_or_default(),
                    segment_duration: segment_duration__.unwrap_or_default(),
                    filename_suffix: filename_suffix__.unwrap_or_default(),
                    disable_manifest: disable_manifest__.unwrap_or_default(),
                    output: output__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.SegmentedFileOutput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SegmentedFileProtocol {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::DefaultSegmentedFileProtocol => "DEFAULT_SEGMENTED_FILE_PROTOCOL",
            Self::HlsProtocol => "HLS_PROTOCOL",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for SegmentedFileProtocol {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "DEFAULT_SEGMENTED_FILE_PROTOCOL",
            "HLS_PROTOCOL",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SegmentedFileProtocol;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(SegmentedFileProtocol::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(SegmentedFileProtocol::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "DEFAULT_SEGMENTED_FILE_PROTOCOL" => Ok(SegmentedFileProtocol::DefaultSegmentedFileProtocol),
                    "HLS_PROTOCOL" => Ok(SegmentedFileProtocol::HlsProtocol),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for SegmentedFileSuffix {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Index => "INDEX",
            Self::Timestamp => "TIMESTAMP",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for SegmentedFileSuffix {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "INDEX",
            "TIMESTAMP",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SegmentedFileSuffix;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(SegmentedFileSuffix::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(SegmentedFileSuffix::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "INDEX" => Ok(SegmentedFileSuffix::Index),
                    "TIMESTAMP" => Ok(SegmentedFileSuffix::Timestamp),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for SegmentsInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.playlist_name.is_empty() {
            len += 1;
        }
        if !self.live_playlist_name.is_empty() {
            len += 1;
        }
        if self.duration != 0 {
            len += 1;
        }
        if self.size != 0 {
            len += 1;
        }
        if !self.playlist_location.is_empty() {
            len += 1;
        }
        if !self.live_playlist_location.is_empty() {
            len += 1;
        }
        if self.segment_count != 0 {
            len += 1;
        }
        if self.started_at != 0 {
            len += 1;
        }
        if self.ended_at != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SegmentsInfo", len)?;
        if !self.playlist_name.is_empty() {
            struct_ser.serialize_field("playlistName", &self.playlist_name)?;
        }
        if !self.live_playlist_name.is_empty() {
            struct_ser.serialize_field("livePlaylistName", &self.live_playlist_name)?;
        }
        if self.duration != 0 {
            struct_ser.serialize_field("duration", ToString::to_string(&self.duration).as_str())?;
        }
        if self.size != 0 {
            struct_ser.serialize_field("size", ToString::to_string(&self.size).as_str())?;
        }
        if !self.playlist_location.is_empty() {
            struct_ser.serialize_field("playlistLocation", &self.playlist_location)?;
        }
        if !self.live_playlist_location.is_empty() {
            struct_ser.serialize_field("livePlaylistLocation", &self.live_playlist_location)?;
        }
        if self.segment_count != 0 {
            struct_ser.serialize_field("segmentCount", ToString::to_string(&self.segment_count).as_str())?;
        }
        if self.started_at != 0 {
            struct_ser.serialize_field("startedAt", ToString::to_string(&self.started_at).as_str())?;
        }
        if self.ended_at != 0 {
            struct_ser.serialize_field("endedAt", ToString::to_string(&self.ended_at).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SegmentsInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "playlist_name",
            "playlistName",
            "live_playlist_name",
            "livePlaylistName",
            "duration",
            "size",
            "playlist_location",
            "playlistLocation",
            "live_playlist_location",
            "livePlaylistLocation",
            "segment_count",
            "segmentCount",
            "started_at",
            "startedAt",
            "ended_at",
            "endedAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PlaylistName,
            LivePlaylistName,
            Duration,
            Size,
            PlaylistLocation,
            LivePlaylistLocation,
            SegmentCount,
            StartedAt,
            EndedAt,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "playlistName" | "playlist_name" => Ok(GeneratedField::PlaylistName),
                            "livePlaylistName" | "live_playlist_name" => Ok(GeneratedField::LivePlaylistName),
                            "duration" => Ok(GeneratedField::Duration),
                            "size" => Ok(GeneratedField::Size),
                            "playlistLocation" | "playlist_location" => Ok(GeneratedField::PlaylistLocation),
                            "livePlaylistLocation" | "live_playlist_location" => Ok(GeneratedField::LivePlaylistLocation),
                            "segmentCount" | "segment_count" => Ok(GeneratedField::SegmentCount),
                            "startedAt" | "started_at" => Ok(GeneratedField::StartedAt),
                            "endedAt" | "ended_at" => Ok(GeneratedField::EndedAt),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SegmentsInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SegmentsInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SegmentsInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut playlist_name__ = None;
                let mut live_playlist_name__ = None;
                let mut duration__ = None;
                let mut size__ = None;
                let mut playlist_location__ = None;
                let mut live_playlist_location__ = None;
                let mut segment_count__ = None;
                let mut started_at__ = None;
                let mut ended_at__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::PlaylistName => {
                            if playlist_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("playlistName"));
                            }
                            playlist_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::LivePlaylistName => {
                            if live_playlist_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("livePlaylistName"));
                            }
                            live_playlist_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Duration => {
                            if duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("duration"));
                            }
                            duration__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Size => {
                            if size__.is_some() {
                                return Err(serde::de::Error::duplicate_field("size"));
                            }
                            size__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PlaylistLocation => {
                            if playlist_location__.is_some() {
                                return Err(serde::de::Error::duplicate_field("playlistLocation"));
                            }
                            playlist_location__ = Some(map.next_value()?);
                        }
                        GeneratedField::LivePlaylistLocation => {
                            if live_playlist_location__.is_some() {
                                return Err(serde::de::Error::duplicate_field("livePlaylistLocation"));
                            }
                            live_playlist_location__ = Some(map.next_value()?);
                        }
                        GeneratedField::SegmentCount => {
                            if segment_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segmentCount"));
                            }
                            segment_count__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::StartedAt => {
                            if started_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startedAt"));
                            }
                            started_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndedAt => {
                            if ended_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endedAt"));
                            }
                            ended_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(SegmentsInfo {
                    playlist_name: playlist_name__.unwrap_or_default(),
                    live_playlist_name: live_playlist_name__.unwrap_or_default(),
                    duration: duration__.unwrap_or_default(),
                    size: size__.unwrap_or_default(),
                    playlist_location: playlist_location__.unwrap_or_default(),
                    live_playlist_location: live_playlist_location__.unwrap_or_default(),
                    segment_count: segment_count__.unwrap_or_default(),
                    started_at: started_at__.unwrap_or_default(),
                    ended_at: ended_at__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SegmentsInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SendDataRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room.is_empty() {
            len += 1;
        }
        if !self.data.is_empty() {
            len += 1;
        }
        if self.kind != 0 {
            len += 1;
        }
        if !self.destination_sids.is_empty() {
            len += 1;
        }
        if !self.destination_identities.is_empty() {
            len += 1;
        }
        if self.topic.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SendDataRequest", len)?;
        if !self.room.is_empty() {
            struct_ser.serialize_field("room", &self.room)?;
        }
        if !self.data.is_empty() {
            struct_ser.serialize_field("data", pbjson::private::base64::encode(&self.data).as_str())?;
        }
        if self.kind != 0 {
            let v = data_packet::Kind::from_i32(self.kind)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.kind)))?;
            struct_ser.serialize_field("kind", &v)?;
        }
        if !self.destination_sids.is_empty() {
            struct_ser.serialize_field("destinationSids", &self.destination_sids)?;
        }
        if !self.destination_identities.is_empty() {
            struct_ser.serialize_field("destinationIdentities", &self.destination_identities)?;
        }
        if let Some(v) = self.topic.as_ref() {
            struct_ser.serialize_field("topic", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SendDataRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
            "data",
            "kind",
            "destination_sids",
            "destinationSids",
            "destination_identities",
            "destinationIdentities",
            "topic",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
            Data,
            Kind,
            DestinationSids,
            DestinationIdentities,
            Topic,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            "data" => Ok(GeneratedField::Data),
                            "kind" => Ok(GeneratedField::Kind),
                            "destinationSids" | "destination_sids" => Ok(GeneratedField::DestinationSids),
                            "destinationIdentities" | "destination_identities" => Ok(GeneratedField::DestinationIdentities),
                            "topic" => Ok(GeneratedField::Topic),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SendDataRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SendDataRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SendDataRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                let mut data__ = None;
                let mut kind__ = None;
                let mut destination_sids__ = None;
                let mut destination_identities__ = None;
                let mut topic__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = Some(map.next_value()?);
                        }
                        GeneratedField::Data => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("data"));
                            }
                            data__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Kind => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("kind"));
                            }
                            kind__ = Some(map.next_value::<data_packet::Kind>()? as i32);
                        }
                        GeneratedField::DestinationSids => {
                            if destination_sids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("destinationSids"));
                            }
                            destination_sids__ = Some(map.next_value()?);
                        }
                        GeneratedField::DestinationIdentities => {
                            if destination_identities__.is_some() {
                                return Err(serde::de::Error::duplicate_field("destinationIdentities"));
                            }
                            destination_identities__ = Some(map.next_value()?);
                        }
                        GeneratedField::Topic => {
                            if topic__.is_some() {
                                return Err(serde::de::Error::duplicate_field("topic"));
                            }
                            topic__ = map.next_value()?;
                        }
                    }
                }
                Ok(SendDataRequest {
                    room: room__.unwrap_or_default(),
                    data: data__.unwrap_or_default(),
                    kind: kind__.unwrap_or_default(),
                    destination_sids: destination_sids__.unwrap_or_default(),
                    destination_identities: destination_identities__.unwrap_or_default(),
                    topic: topic__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.SendDataRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SendDataResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.SendDataResponse", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SendDataResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Err(serde::de::Error::unknown_field(value, FIELDS))
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SendDataResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SendDataResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SendDataResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map.next_key::<GeneratedField>()?.is_some() {
                    let _ = map.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(SendDataResponse {
                })
            }
        }
        deserializer.deserialize_struct("livekit.SendDataResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ServerInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.edition != 0 {
            len += 1;
        }
        if !self.version.is_empty() {
            len += 1;
        }
        if self.protocol != 0 {
            len += 1;
        }
        if !self.region.is_empty() {
            len += 1;
        }
        if !self.node_id.is_empty() {
            len += 1;
        }
        if !self.debug_info.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.ServerInfo", len)?;
        if self.edition != 0 {
            let v = server_info::Edition::from_i32(self.edition)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.edition)))?;
            struct_ser.serialize_field("edition", &v)?;
        }
        if !self.version.is_empty() {
            struct_ser.serialize_field("version", &self.version)?;
        }
        if self.protocol != 0 {
            struct_ser.serialize_field("protocol", &self.protocol)?;
        }
        if !self.region.is_empty() {
            struct_ser.serialize_field("region", &self.region)?;
        }
        if !self.node_id.is_empty() {
            struct_ser.serialize_field("nodeId", &self.node_id)?;
        }
        if !self.debug_info.is_empty() {
            struct_ser.serialize_field("debugInfo", &self.debug_info)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ServerInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "edition",
            "version",
            "protocol",
            "region",
            "node_id",
            "nodeId",
            "debug_info",
            "debugInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Edition,
            Version,
            Protocol,
            Region,
            NodeId,
            DebugInfo,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "edition" => Ok(GeneratedField::Edition),
                            "version" => Ok(GeneratedField::Version),
                            "protocol" => Ok(GeneratedField::Protocol),
                            "region" => Ok(GeneratedField::Region),
                            "nodeId" | "node_id" => Ok(GeneratedField::NodeId),
                            "debugInfo" | "debug_info" => Ok(GeneratedField::DebugInfo),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ServerInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.ServerInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ServerInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut edition__ = None;
                let mut version__ = None;
                let mut protocol__ = None;
                let mut region__ = None;
                let mut node_id__ = None;
                let mut debug_info__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Edition => {
                            if edition__.is_some() {
                                return Err(serde::de::Error::duplicate_field("edition"));
                            }
                            edition__ = Some(map.next_value::<server_info::Edition>()? as i32);
                        }
                        GeneratedField::Version => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("version"));
                            }
                            version__ = Some(map.next_value()?);
                        }
                        GeneratedField::Protocol => {
                            if protocol__.is_some() {
                                return Err(serde::de::Error::duplicate_field("protocol"));
                            }
                            protocol__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Region => {
                            if region__.is_some() {
                                return Err(serde::de::Error::duplicate_field("region"));
                            }
                            region__ = Some(map.next_value()?);
                        }
                        GeneratedField::NodeId => {
                            if node_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nodeId"));
                            }
                            node_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::DebugInfo => {
                            if debug_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("debugInfo"));
                            }
                            debug_info__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ServerInfo {
                    edition: edition__.unwrap_or_default(),
                    version: version__.unwrap_or_default(),
                    protocol: protocol__.unwrap_or_default(),
                    region: region__.unwrap_or_default(),
                    node_id: node_id__.unwrap_or_default(),
                    debug_info: debug_info__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.ServerInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for server_info::Edition {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Standard => "Standard",
            Self::Cloud => "Cloud",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for server_info::Edition {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "Standard",
            "Cloud",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = server_info::Edition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(server_info::Edition::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(server_info::Edition::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "Standard" => Ok(server_info::Edition::Standard),
                    "Cloud" => Ok(server_info::Edition::Cloud),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for SessionDescription {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.r#type.is_empty() {
            len += 1;
        }
        if !self.sdp.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SessionDescription", len)?;
        if !self.r#type.is_empty() {
            struct_ser.serialize_field("type", &self.r#type)?;
        }
        if !self.sdp.is_empty() {
            struct_ser.serialize_field("sdp", &self.sdp)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SessionDescription {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "type",
            "sdp",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Type,
            Sdp,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "type" => Ok(GeneratedField::Type),
                            "sdp" => Ok(GeneratedField::Sdp),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SessionDescription;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SessionDescription")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SessionDescription, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut r#type__ = None;
                let mut sdp__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map.next_value()?);
                        }
                        GeneratedField::Sdp => {
                            if sdp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sdp"));
                            }
                            sdp__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SessionDescription {
                    r#type: r#type__.unwrap_or_default(),
                    sdp: sdp__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SessionDescription", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SignalRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.message.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SignalRequest", len)?;
        if let Some(v) = self.message.as_ref() {
            match v {
                signal_request::Message::Offer(v) => {
                    struct_ser.serialize_field("offer", v)?;
                }
                signal_request::Message::Answer(v) => {
                    struct_ser.serialize_field("answer", v)?;
                }
                signal_request::Message::Trickle(v) => {
                    struct_ser.serialize_field("trickle", v)?;
                }
                signal_request::Message::AddTrack(v) => {
                    struct_ser.serialize_field("addTrack", v)?;
                }
                signal_request::Message::Mute(v) => {
                    struct_ser.serialize_field("mute", v)?;
                }
                signal_request::Message::Subscription(v) => {
                    struct_ser.serialize_field("subscription", v)?;
                }
                signal_request::Message::TrackSetting(v) => {
                    struct_ser.serialize_field("trackSetting", v)?;
                }
                signal_request::Message::Leave(v) => {
                    struct_ser.serialize_field("leave", v)?;
                }
                signal_request::Message::UpdateLayers(v) => {
                    struct_ser.serialize_field("updateLayers", v)?;
                }
                signal_request::Message::SubscriptionPermission(v) => {
                    struct_ser.serialize_field("subscriptionPermission", v)?;
                }
                signal_request::Message::SyncState(v) => {
                    struct_ser.serialize_field("syncState", v)?;
                }
                signal_request::Message::Simulate(v) => {
                    struct_ser.serialize_field("simulate", v)?;
                }
                signal_request::Message::Ping(v) => {
                    struct_ser.serialize_field("ping", ToString::to_string(&v).as_str())?;
                }
                signal_request::Message::UpdateMetadata(v) => {
                    struct_ser.serialize_field("updateMetadata", v)?;
                }
                signal_request::Message::PingReq(v) => {
                    struct_ser.serialize_field("pingReq", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SignalRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "offer",
            "answer",
            "trickle",
            "add_track",
            "addTrack",
            "mute",
            "subscription",
            "track_setting",
            "trackSetting",
            "leave",
            "update_layers",
            "updateLayers",
            "subscription_permission",
            "subscriptionPermission",
            "sync_state",
            "syncState",
            "simulate",
            "ping",
            "update_metadata",
            "updateMetadata",
            "ping_req",
            "pingReq",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Offer,
            Answer,
            Trickle,
            AddTrack,
            Mute,
            Subscription,
            TrackSetting,
            Leave,
            UpdateLayers,
            SubscriptionPermission,
            SyncState,
            Simulate,
            Ping,
            UpdateMetadata,
            PingReq,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "offer" => Ok(GeneratedField::Offer),
                            "answer" => Ok(GeneratedField::Answer),
                            "trickle" => Ok(GeneratedField::Trickle),
                            "addTrack" | "add_track" => Ok(GeneratedField::AddTrack),
                            "mute" => Ok(GeneratedField::Mute),
                            "subscription" => Ok(GeneratedField::Subscription),
                            "trackSetting" | "track_setting" => Ok(GeneratedField::TrackSetting),
                            "leave" => Ok(GeneratedField::Leave),
                            "updateLayers" | "update_layers" => Ok(GeneratedField::UpdateLayers),
                            "subscriptionPermission" | "subscription_permission" => Ok(GeneratedField::SubscriptionPermission),
                            "syncState" | "sync_state" => Ok(GeneratedField::SyncState),
                            "simulate" => Ok(GeneratedField::Simulate),
                            "ping" => Ok(GeneratedField::Ping),
                            "updateMetadata" | "update_metadata" => Ok(GeneratedField::UpdateMetadata),
                            "pingReq" | "ping_req" => Ok(GeneratedField::PingReq),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SignalRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SignalRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SignalRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Offer => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("offer"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::Offer)
;
                        }
                        GeneratedField::Answer => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("answer"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::Answer)
;
                        }
                        GeneratedField::Trickle => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trickle"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::Trickle)
;
                        }
                        GeneratedField::AddTrack => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addTrack"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::AddTrack)
;
                        }
                        GeneratedField::Mute => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mute"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::Mute)
;
                        }
                        GeneratedField::Subscription => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscription"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::Subscription)
;
                        }
                        GeneratedField::TrackSetting => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSetting"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::TrackSetting)
;
                        }
                        GeneratedField::Leave => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leave"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::Leave)
;
                        }
                        GeneratedField::UpdateLayers => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updateLayers"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::UpdateLayers)
;
                        }
                        GeneratedField::SubscriptionPermission => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscriptionPermission"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::SubscriptionPermission)
;
                        }
                        GeneratedField::SyncState => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("syncState"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::SyncState)
;
                        }
                        GeneratedField::Simulate => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("simulate"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::Simulate)
;
                        }
                        GeneratedField::Ping => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ping"));
                            }
                            message__ = map.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| signal_request::Message::Ping(x.0));
                        }
                        GeneratedField::UpdateMetadata => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updateMetadata"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::UpdateMetadata)
;
                        }
                        GeneratedField::PingReq => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pingReq"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_request::Message::PingReq)
;
                        }
                    }
                }
                Ok(SignalRequest {
                    message: message__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.SignalRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SignalResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.message.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SignalResponse", len)?;
        if let Some(v) = self.message.as_ref() {
            match v {
                signal_response::Message::Join(v) => {
                    struct_ser.serialize_field("join", v)?;
                }
                signal_response::Message::Answer(v) => {
                    struct_ser.serialize_field("answer", v)?;
                }
                signal_response::Message::Offer(v) => {
                    struct_ser.serialize_field("offer", v)?;
                }
                signal_response::Message::Trickle(v) => {
                    struct_ser.serialize_field("trickle", v)?;
                }
                signal_response::Message::Update(v) => {
                    struct_ser.serialize_field("update", v)?;
                }
                signal_response::Message::TrackPublished(v) => {
                    struct_ser.serialize_field("trackPublished", v)?;
                }
                signal_response::Message::Leave(v) => {
                    struct_ser.serialize_field("leave", v)?;
                }
                signal_response::Message::Mute(v) => {
                    struct_ser.serialize_field("mute", v)?;
                }
                signal_response::Message::SpeakersChanged(v) => {
                    struct_ser.serialize_field("speakersChanged", v)?;
                }
                signal_response::Message::RoomUpdate(v) => {
                    struct_ser.serialize_field("roomUpdate", v)?;
                }
                signal_response::Message::ConnectionQuality(v) => {
                    struct_ser.serialize_field("connectionQuality", v)?;
                }
                signal_response::Message::StreamStateUpdate(v) => {
                    struct_ser.serialize_field("streamStateUpdate", v)?;
                }
                signal_response::Message::SubscribedQualityUpdate(v) => {
                    struct_ser.serialize_field("subscribedQualityUpdate", v)?;
                }
                signal_response::Message::SubscriptionPermissionUpdate(v) => {
                    struct_ser.serialize_field("subscriptionPermissionUpdate", v)?;
                }
                signal_response::Message::RefreshToken(v) => {
                    struct_ser.serialize_field("refreshToken", v)?;
                }
                signal_response::Message::TrackUnpublished(v) => {
                    struct_ser.serialize_field("trackUnpublished", v)?;
                }
                signal_response::Message::Pong(v) => {
                    struct_ser.serialize_field("pong", ToString::to_string(&v).as_str())?;
                }
                signal_response::Message::Reconnect(v) => {
                    struct_ser.serialize_field("reconnect", v)?;
                }
                signal_response::Message::PongResp(v) => {
                    struct_ser.serialize_field("pongResp", v)?;
                }
                signal_response::Message::SubscriptionResponse(v) => {
                    struct_ser.serialize_field("subscriptionResponse", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SignalResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "join",
            "answer",
            "offer",
            "trickle",
            "update",
            "track_published",
            "trackPublished",
            "leave",
            "mute",
            "speakers_changed",
            "speakersChanged",
            "room_update",
            "roomUpdate",
            "connection_quality",
            "connectionQuality",
            "stream_state_update",
            "streamStateUpdate",
            "subscribed_quality_update",
            "subscribedQualityUpdate",
            "subscription_permission_update",
            "subscriptionPermissionUpdate",
            "refresh_token",
            "refreshToken",
            "track_unpublished",
            "trackUnpublished",
            "pong",
            "reconnect",
            "pong_resp",
            "pongResp",
            "subscription_response",
            "subscriptionResponse",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Join,
            Answer,
            Offer,
            Trickle,
            Update,
            TrackPublished,
            Leave,
            Mute,
            SpeakersChanged,
            RoomUpdate,
            ConnectionQuality,
            StreamStateUpdate,
            SubscribedQualityUpdate,
            SubscriptionPermissionUpdate,
            RefreshToken,
            TrackUnpublished,
            Pong,
            Reconnect,
            PongResp,
            SubscriptionResponse,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "join" => Ok(GeneratedField::Join),
                            "answer" => Ok(GeneratedField::Answer),
                            "offer" => Ok(GeneratedField::Offer),
                            "trickle" => Ok(GeneratedField::Trickle),
                            "update" => Ok(GeneratedField::Update),
                            "trackPublished" | "track_published" => Ok(GeneratedField::TrackPublished),
                            "leave" => Ok(GeneratedField::Leave),
                            "mute" => Ok(GeneratedField::Mute),
                            "speakersChanged" | "speakers_changed" => Ok(GeneratedField::SpeakersChanged),
                            "roomUpdate" | "room_update" => Ok(GeneratedField::RoomUpdate),
                            "connectionQuality" | "connection_quality" => Ok(GeneratedField::ConnectionQuality),
                            "streamStateUpdate" | "stream_state_update" => Ok(GeneratedField::StreamStateUpdate),
                            "subscribedQualityUpdate" | "subscribed_quality_update" => Ok(GeneratedField::SubscribedQualityUpdate),
                            "subscriptionPermissionUpdate" | "subscription_permission_update" => Ok(GeneratedField::SubscriptionPermissionUpdate),
                            "refreshToken" | "refresh_token" => Ok(GeneratedField::RefreshToken),
                            "trackUnpublished" | "track_unpublished" => Ok(GeneratedField::TrackUnpublished),
                            "pong" => Ok(GeneratedField::Pong),
                            "reconnect" => Ok(GeneratedField::Reconnect),
                            "pongResp" | "pong_resp" => Ok(GeneratedField::PongResp),
                            "subscriptionResponse" | "subscription_response" => Ok(GeneratedField::SubscriptionResponse),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SignalResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SignalResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SignalResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Join => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("join"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::Join)
;
                        }
                        GeneratedField::Answer => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("answer"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::Answer)
;
                        }
                        GeneratedField::Offer => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("offer"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::Offer)
;
                        }
                        GeneratedField::Trickle => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trickle"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::Trickle)
;
                        }
                        GeneratedField::Update => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("update"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::Update)
;
                        }
                        GeneratedField::TrackPublished => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackPublished"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::TrackPublished)
;
                        }
                        GeneratedField::Leave => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("leave"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::Leave)
;
                        }
                        GeneratedField::Mute => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mute"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::Mute)
;
                        }
                        GeneratedField::SpeakersChanged => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("speakersChanged"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::SpeakersChanged)
;
                        }
                        GeneratedField::RoomUpdate => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomUpdate"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::RoomUpdate)
;
                        }
                        GeneratedField::ConnectionQuality => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("connectionQuality"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::ConnectionQuality)
;
                        }
                        GeneratedField::StreamStateUpdate => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("streamStateUpdate"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::StreamStateUpdate)
;
                        }
                        GeneratedField::SubscribedQualityUpdate => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscribedQualityUpdate"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::SubscribedQualityUpdate)
;
                        }
                        GeneratedField::SubscriptionPermissionUpdate => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscriptionPermissionUpdate"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::SubscriptionPermissionUpdate)
;
                        }
                        GeneratedField::RefreshToken => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("refreshToken"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::RefreshToken);
                        }
                        GeneratedField::TrackUnpublished => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackUnpublished"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::TrackUnpublished)
;
                        }
                        GeneratedField::Pong => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pong"));
                            }
                            message__ = map.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| signal_response::Message::Pong(x.0));
                        }
                        GeneratedField::Reconnect => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reconnect"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::Reconnect)
;
                        }
                        GeneratedField::PongResp => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pongResp"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::PongResp)
;
                        }
                        GeneratedField::SubscriptionResponse => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscriptionResponse"));
                            }
                            message__ = map.next_value::<::std::option::Option<_>>()?.map(signal_response::Message::SubscriptionResponse)
;
                        }
                    }
                }
                Ok(SignalResponse {
                    message: message__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.SignalResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SignalTarget {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Publisher => "PUBLISHER",
            Self::Subscriber => "SUBSCRIBER",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for SignalTarget {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PUBLISHER",
            "SUBSCRIBER",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SignalTarget;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(SignalTarget::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(SignalTarget::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "PUBLISHER" => Ok(SignalTarget::Publisher),
                    "SUBSCRIBER" => Ok(SignalTarget::Subscriber),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for SimulateScenario {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.scenario.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SimulateScenario", len)?;
        if let Some(v) = self.scenario.as_ref() {
            match v {
                simulate_scenario::Scenario::SpeakerUpdate(v) => {
                    struct_ser.serialize_field("speakerUpdate", v)?;
                }
                simulate_scenario::Scenario::NodeFailure(v) => {
                    struct_ser.serialize_field("nodeFailure", v)?;
                }
                simulate_scenario::Scenario::Migration(v) => {
                    struct_ser.serialize_field("migration", v)?;
                }
                simulate_scenario::Scenario::ServerLeave(v) => {
                    struct_ser.serialize_field("serverLeave", v)?;
                }
                simulate_scenario::Scenario::SwitchCandidateProtocol(v) => {
                    let v = CandidateProtocol::from_i32(*v)
                        .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("switchCandidateProtocol", &v)?;
                }
                simulate_scenario::Scenario::SubscriberBandwidth(v) => {
                    struct_ser.serialize_field("subscriberBandwidth", ToString::to_string(&v).as_str())?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SimulateScenario {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "speaker_update",
            "speakerUpdate",
            "node_failure",
            "nodeFailure",
            "migration",
            "server_leave",
            "serverLeave",
            "switch_candidate_protocol",
            "switchCandidateProtocol",
            "subscriber_bandwidth",
            "subscriberBandwidth",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SpeakerUpdate,
            NodeFailure,
            Migration,
            ServerLeave,
            SwitchCandidateProtocol,
            SubscriberBandwidth,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "speakerUpdate" | "speaker_update" => Ok(GeneratedField::SpeakerUpdate),
                            "nodeFailure" | "node_failure" => Ok(GeneratedField::NodeFailure),
                            "migration" => Ok(GeneratedField::Migration),
                            "serverLeave" | "server_leave" => Ok(GeneratedField::ServerLeave),
                            "switchCandidateProtocol" | "switch_candidate_protocol" => Ok(GeneratedField::SwitchCandidateProtocol),
                            "subscriberBandwidth" | "subscriber_bandwidth" => Ok(GeneratedField::SubscriberBandwidth),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SimulateScenario;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SimulateScenario")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SimulateScenario, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut scenario__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::SpeakerUpdate => {
                            if scenario__.is_some() {
                                return Err(serde::de::Error::duplicate_field("speakerUpdate"));
                            }
                            scenario__ = map.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| simulate_scenario::Scenario::SpeakerUpdate(x.0));
                        }
                        GeneratedField::NodeFailure => {
                            if scenario__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nodeFailure"));
                            }
                            scenario__ = map.next_value::<::std::option::Option<_>>()?.map(simulate_scenario::Scenario::NodeFailure);
                        }
                        GeneratedField::Migration => {
                            if scenario__.is_some() {
                                return Err(serde::de::Error::duplicate_field("migration"));
                            }
                            scenario__ = map.next_value::<::std::option::Option<_>>()?.map(simulate_scenario::Scenario::Migration);
                        }
                        GeneratedField::ServerLeave => {
                            if scenario__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverLeave"));
                            }
                            scenario__ = map.next_value::<::std::option::Option<_>>()?.map(simulate_scenario::Scenario::ServerLeave);
                        }
                        GeneratedField::SwitchCandidateProtocol => {
                            if scenario__.is_some() {
                                return Err(serde::de::Error::duplicate_field("switchCandidateProtocol"));
                            }
                            scenario__ = map.next_value::<::std::option::Option<CandidateProtocol>>()?.map(|x| simulate_scenario::Scenario::SwitchCandidateProtocol(x as i32));
                        }
                        GeneratedField::SubscriberBandwidth => {
                            if scenario__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscriberBandwidth"));
                            }
                            scenario__ = map.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| simulate_scenario::Scenario::SubscriberBandwidth(x.0));
                        }
                    }
                }
                Ok(SimulateScenario {
                    scenario: scenario__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.SimulateScenario", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SimulcastCodec {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.codec.is_empty() {
            len += 1;
        }
        if !self.cid.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SimulcastCodec", len)?;
        if !self.codec.is_empty() {
            struct_ser.serialize_field("codec", &self.codec)?;
        }
        if !self.cid.is_empty() {
            struct_ser.serialize_field("cid", &self.cid)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SimulcastCodec {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "codec",
            "cid",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Codec,
            Cid,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "codec" => Ok(GeneratedField::Codec),
                            "cid" => Ok(GeneratedField::Cid),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SimulcastCodec;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SimulcastCodec")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SimulcastCodec, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut codec__ = None;
                let mut cid__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Codec => {
                            if codec__.is_some() {
                                return Err(serde::de::Error::duplicate_field("codec"));
                            }
                            codec__ = Some(map.next_value()?);
                        }
                        GeneratedField::Cid => {
                            if cid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("cid"));
                            }
                            cid__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SimulcastCodec {
                    codec: codec__.unwrap_or_default(),
                    cid: cid__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SimulcastCodec", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SimulcastCodecInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.mime_type.is_empty() {
            len += 1;
        }
        if !self.mid.is_empty() {
            len += 1;
        }
        if !self.cid.is_empty() {
            len += 1;
        }
        if !self.layers.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SimulcastCodecInfo", len)?;
        if !self.mime_type.is_empty() {
            struct_ser.serialize_field("mimeType", &self.mime_type)?;
        }
        if !self.mid.is_empty() {
            struct_ser.serialize_field("mid", &self.mid)?;
        }
        if !self.cid.is_empty() {
            struct_ser.serialize_field("cid", &self.cid)?;
        }
        if !self.layers.is_empty() {
            struct_ser.serialize_field("layers", &self.layers)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SimulcastCodecInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "mime_type",
            "mimeType",
            "mid",
            "cid",
            "layers",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MimeType,
            Mid,
            Cid,
            Layers,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "mimeType" | "mime_type" => Ok(GeneratedField::MimeType),
                            "mid" => Ok(GeneratedField::Mid),
                            "cid" => Ok(GeneratedField::Cid),
                            "layers" => Ok(GeneratedField::Layers),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SimulcastCodecInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SimulcastCodecInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SimulcastCodecInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut mime_type__ = None;
                let mut mid__ = None;
                let mut cid__ = None;
                let mut layers__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::MimeType => {
                            if mime_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mimeType"));
                            }
                            mime_type__ = Some(map.next_value()?);
                        }
                        GeneratedField::Mid => {
                            if mid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mid"));
                            }
                            mid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Cid => {
                            if cid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("cid"));
                            }
                            cid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Layers => {
                            if layers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("layers"));
                            }
                            layers__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SimulcastCodecInfo {
                    mime_type: mime_type__.unwrap_or_default(),
                    mid: mid__.unwrap_or_default(),
                    cid: cid__.unwrap_or_default(),
                    layers: layers__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SimulcastCodecInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SpeakerInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.sid.is_empty() {
            len += 1;
        }
        if self.level != 0. {
            len += 1;
        }
        if self.active {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SpeakerInfo", len)?;
        if !self.sid.is_empty() {
            struct_ser.serialize_field("sid", &self.sid)?;
        }
        if self.level != 0. {
            struct_ser.serialize_field("level", &self.level)?;
        }
        if self.active {
            struct_ser.serialize_field("active", &self.active)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SpeakerInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sid",
            "level",
            "active",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Sid,
            Level,
            Active,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "sid" => Ok(GeneratedField::Sid),
                            "level" => Ok(GeneratedField::Level),
                            "active" => Ok(GeneratedField::Active),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SpeakerInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SpeakerInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SpeakerInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sid__ = None;
                let mut level__ = None;
                let mut active__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Sid => {
                            if sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sid"));
                            }
                            sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Level => {
                            if level__.is_some() {
                                return Err(serde::de::Error::duplicate_field("level"));
                            }
                            level__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Active => {
                            if active__.is_some() {
                                return Err(serde::de::Error::duplicate_field("active"));
                            }
                            active__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SpeakerInfo {
                    sid: sid__.unwrap_or_default(),
                    level: level__.unwrap_or_default(),
                    active: active__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SpeakerInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SpeakersChanged {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.speakers.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SpeakersChanged", len)?;
        if !self.speakers.is_empty() {
            struct_ser.serialize_field("speakers", &self.speakers)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SpeakersChanged {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "speakers",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Speakers,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "speakers" => Ok(GeneratedField::Speakers),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SpeakersChanged;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SpeakersChanged")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SpeakersChanged, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut speakers__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Speakers => {
                            if speakers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("speakers"));
                            }
                            speakers__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SpeakersChanged {
                    speakers: speakers__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SpeakersChanged", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StopEgressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.egress_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.StopEgressRequest", len)?;
        if !self.egress_id.is_empty() {
            struct_ser.serialize_field("egressId", &self.egress_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StopEgressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "egress_id",
            "egressId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EgressId,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "egressId" | "egress_id" => Ok(GeneratedField::EgressId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StopEgressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.StopEgressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<StopEgressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut egress_id__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::EgressId => {
                            if egress_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("egressId"));
                            }
                            egress_id__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(StopEgressRequest {
                    egress_id: egress_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.StopEgressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StreamInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.url.is_empty() {
            len += 1;
        }
        if self.started_at != 0 {
            len += 1;
        }
        if self.ended_at != 0 {
            len += 1;
        }
        if self.duration != 0 {
            len += 1;
        }
        if self.status != 0 {
            len += 1;
        }
        if !self.error.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.StreamInfo", len)?;
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        if self.started_at != 0 {
            struct_ser.serialize_field("startedAt", ToString::to_string(&self.started_at).as_str())?;
        }
        if self.ended_at != 0 {
            struct_ser.serialize_field("endedAt", ToString::to_string(&self.ended_at).as_str())?;
        }
        if self.duration != 0 {
            struct_ser.serialize_field("duration", ToString::to_string(&self.duration).as_str())?;
        }
        if self.status != 0 {
            let v = stream_info::Status::from_i32(self.status)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.status)))?;
            struct_ser.serialize_field("status", &v)?;
        }
        if !self.error.is_empty() {
            struct_ser.serialize_field("error", &self.error)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StreamInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "url",
            "started_at",
            "startedAt",
            "ended_at",
            "endedAt",
            "duration",
            "status",
            "error",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Url,
            StartedAt,
            EndedAt,
            Duration,
            Status,
            Error,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "url" => Ok(GeneratedField::Url),
                            "startedAt" | "started_at" => Ok(GeneratedField::StartedAt),
                            "endedAt" | "ended_at" => Ok(GeneratedField::EndedAt),
                            "duration" => Ok(GeneratedField::Duration),
                            "status" => Ok(GeneratedField::Status),
                            "error" => Ok(GeneratedField::Error),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StreamInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.StreamInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<StreamInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut url__ = None;
                let mut started_at__ = None;
                let mut ended_at__ = None;
                let mut duration__ = None;
                let mut status__ = None;
                let mut error__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map.next_value()?);
                        }
                        GeneratedField::StartedAt => {
                            if started_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startedAt"));
                            }
                            started_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndedAt => {
                            if ended_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endedAt"));
                            }
                            ended_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Duration => {
                            if duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("duration"));
                            }
                            duration__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Status => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("status"));
                            }
                            status__ = Some(map.next_value::<stream_info::Status>()? as i32);
                        }
                        GeneratedField::Error => {
                            if error__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            error__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(StreamInfo {
                    url: url__.unwrap_or_default(),
                    started_at: started_at__.unwrap_or_default(),
                    ended_at: ended_at__.unwrap_or_default(),
                    duration: duration__.unwrap_or_default(),
                    status: status__.unwrap_or_default(),
                    error: error__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.StreamInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for stream_info::Status {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Active => "ACTIVE",
            Self::Finished => "FINISHED",
            Self::Failed => "FAILED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for stream_info::Status {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ACTIVE",
            "FINISHED",
            "FAILED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = stream_info::Status;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(stream_info::Status::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(stream_info::Status::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ACTIVE" => Ok(stream_info::Status::Active),
                    "FINISHED" => Ok(stream_info::Status::Finished),
                    "FAILED" => Ok(stream_info::Status::Failed),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for StreamInfoList {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.info.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.StreamInfoList", len)?;
        if !self.info.is_empty() {
            struct_ser.serialize_field("info", &self.info)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StreamInfoList {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "info",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Info,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "info" => Ok(GeneratedField::Info),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StreamInfoList;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.StreamInfoList")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<StreamInfoList, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut info__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Info => {
                            if info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("info"));
                            }
                            info__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(StreamInfoList {
                    info: info__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.StreamInfoList", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StreamOutput {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.protocol != 0 {
            len += 1;
        }
        if !self.urls.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.StreamOutput", len)?;
        if self.protocol != 0 {
            let v = StreamProtocol::from_i32(self.protocol)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.protocol)))?;
            struct_ser.serialize_field("protocol", &v)?;
        }
        if !self.urls.is_empty() {
            struct_ser.serialize_field("urls", &self.urls)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StreamOutput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "protocol",
            "urls",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Protocol,
            Urls,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "protocol" => Ok(GeneratedField::Protocol),
                            "urls" => Ok(GeneratedField::Urls),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StreamOutput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.StreamOutput")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<StreamOutput, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut protocol__ = None;
                let mut urls__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Protocol => {
                            if protocol__.is_some() {
                                return Err(serde::de::Error::duplicate_field("protocol"));
                            }
                            protocol__ = Some(map.next_value::<StreamProtocol>()? as i32);
                        }
                        GeneratedField::Urls => {
                            if urls__.is_some() {
                                return Err(serde::de::Error::duplicate_field("urls"));
                            }
                            urls__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(StreamOutput {
                    protocol: protocol__.unwrap_or_default(),
                    urls: urls__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.StreamOutput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StreamProtocol {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::DefaultProtocol => "DEFAULT_PROTOCOL",
            Self::Rtmp => "RTMP",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for StreamProtocol {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "DEFAULT_PROTOCOL",
            "RTMP",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StreamProtocol;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(StreamProtocol::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(StreamProtocol::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "DEFAULT_PROTOCOL" => Ok(StreamProtocol::DefaultProtocol),
                    "RTMP" => Ok(StreamProtocol::Rtmp),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for StreamState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Active => "ACTIVE",
            Self::Paused => "PAUSED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for StreamState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ACTIVE",
            "PAUSED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StreamState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(StreamState::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(StreamState::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ACTIVE" => Ok(StreamState::Active),
                    "PAUSED" => Ok(StreamState::Paused),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for StreamStateInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.participant_sid.is_empty() {
            len += 1;
        }
        if !self.track_sid.is_empty() {
            len += 1;
        }
        if self.state != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.StreamStateInfo", len)?;
        if !self.participant_sid.is_empty() {
            struct_ser.serialize_field("participantSid", &self.participant_sid)?;
        }
        if !self.track_sid.is_empty() {
            struct_ser.serialize_field("trackSid", &self.track_sid)?;
        }
        if self.state != 0 {
            let v = StreamState::from_i32(self.state)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.state)))?;
            struct_ser.serialize_field("state", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StreamStateInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "participant_sid",
            "participantSid",
            "track_sid",
            "trackSid",
            "state",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ParticipantSid,
            TrackSid,
            State,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "participantSid" | "participant_sid" => Ok(GeneratedField::ParticipantSid),
                            "trackSid" | "track_sid" => Ok(GeneratedField::TrackSid),
                            "state" => Ok(GeneratedField::State),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StreamStateInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.StreamStateInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<StreamStateInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut participant_sid__ = None;
                let mut track_sid__ = None;
                let mut state__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::ParticipantSid => {
                            if participant_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantSid"));
                            }
                            participant_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::TrackSid => {
                            if track_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSid"));
                            }
                            track_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::State => {
                            if state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("state"));
                            }
                            state__ = Some(map.next_value::<StreamState>()? as i32);
                        }
                    }
                }
                Ok(StreamStateInfo {
                    participant_sid: participant_sid__.unwrap_or_default(),
                    track_sid: track_sid__.unwrap_or_default(),
                    state: state__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.StreamStateInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for StreamStateUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.stream_states.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.StreamStateUpdate", len)?;
        if !self.stream_states.is_empty() {
            struct_ser.serialize_field("streamStates", &self.stream_states)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for StreamStateUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "stream_states",
            "streamStates",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            StreamStates,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "streamStates" | "stream_states" => Ok(GeneratedField::StreamStates),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = StreamStateUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.StreamStateUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<StreamStateUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut stream_states__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::StreamStates => {
                            if stream_states__.is_some() {
                                return Err(serde::de::Error::duplicate_field("streamStates"));
                            }
                            stream_states__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(StreamStateUpdate {
                    stream_states: stream_states__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.StreamStateUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubscribedCodec {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.codec.is_empty() {
            len += 1;
        }
        if !self.qualities.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SubscribedCodec", len)?;
        if !self.codec.is_empty() {
            struct_ser.serialize_field("codec", &self.codec)?;
        }
        if !self.qualities.is_empty() {
            struct_ser.serialize_field("qualities", &self.qualities)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscribedCodec {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "codec",
            "qualities",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Codec,
            Qualities,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "codec" => Ok(GeneratedField::Codec),
                            "qualities" => Ok(GeneratedField::Qualities),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscribedCodec;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SubscribedCodec")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SubscribedCodec, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut codec__ = None;
                let mut qualities__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Codec => {
                            if codec__.is_some() {
                                return Err(serde::de::Error::duplicate_field("codec"));
                            }
                            codec__ = Some(map.next_value()?);
                        }
                        GeneratedField::Qualities => {
                            if qualities__.is_some() {
                                return Err(serde::de::Error::duplicate_field("qualities"));
                            }
                            qualities__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SubscribedCodec {
                    codec: codec__.unwrap_or_default(),
                    qualities: qualities__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SubscribedCodec", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubscribedQuality {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.quality != 0 {
            len += 1;
        }
        if self.enabled {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SubscribedQuality", len)?;
        if self.quality != 0 {
            let v = VideoQuality::from_i32(self.quality)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.quality)))?;
            struct_ser.serialize_field("quality", &v)?;
        }
        if self.enabled {
            struct_ser.serialize_field("enabled", &self.enabled)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscribedQuality {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "quality",
            "enabled",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Quality,
            Enabled,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "quality" => Ok(GeneratedField::Quality),
                            "enabled" => Ok(GeneratedField::Enabled),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscribedQuality;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SubscribedQuality")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SubscribedQuality, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut quality__ = None;
                let mut enabled__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Quality => {
                            if quality__.is_some() {
                                return Err(serde::de::Error::duplicate_field("quality"));
                            }
                            quality__ = Some(map.next_value::<VideoQuality>()? as i32);
                        }
                        GeneratedField::Enabled => {
                            if enabled__.is_some() {
                                return Err(serde::de::Error::duplicate_field("enabled"));
                            }
                            enabled__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SubscribedQuality {
                    quality: quality__.unwrap_or_default(),
                    enabled: enabled__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SubscribedQuality", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubscribedQualityUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.track_sid.is_empty() {
            len += 1;
        }
        if !self.subscribed_qualities.is_empty() {
            len += 1;
        }
        if !self.subscribed_codecs.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SubscribedQualityUpdate", len)?;
        if !self.track_sid.is_empty() {
            struct_ser.serialize_field("trackSid", &self.track_sid)?;
        }
        if !self.subscribed_qualities.is_empty() {
            struct_ser.serialize_field("subscribedQualities", &self.subscribed_qualities)?;
        }
        if !self.subscribed_codecs.is_empty() {
            struct_ser.serialize_field("subscribedCodecs", &self.subscribed_codecs)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscribedQualityUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "track_sid",
            "trackSid",
            "subscribed_qualities",
            "subscribedQualities",
            "subscribed_codecs",
            "subscribedCodecs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TrackSid,
            SubscribedQualities,
            SubscribedCodecs,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "trackSid" | "track_sid" => Ok(GeneratedField::TrackSid),
                            "subscribedQualities" | "subscribed_qualities" => Ok(GeneratedField::SubscribedQualities),
                            "subscribedCodecs" | "subscribed_codecs" => Ok(GeneratedField::SubscribedCodecs),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscribedQualityUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SubscribedQualityUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SubscribedQualityUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut track_sid__ = None;
                let mut subscribed_qualities__ = None;
                let mut subscribed_codecs__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::TrackSid => {
                            if track_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSid"));
                            }
                            track_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::SubscribedQualities => {
                            if subscribed_qualities__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscribedQualities"));
                            }
                            subscribed_qualities__ = Some(map.next_value()?);
                        }
                        GeneratedField::SubscribedCodecs => {
                            if subscribed_codecs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscribedCodecs"));
                            }
                            subscribed_codecs__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SubscribedQualityUpdate {
                    track_sid: track_sid__.unwrap_or_default(),
                    subscribed_qualities: subscribed_qualities__.unwrap_or_default(),
                    subscribed_codecs: subscribed_codecs__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SubscribedQualityUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubscriptionError {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::SeUnknown => "SE_UNKNOWN",
            Self::SeCodecUnsupported => "SE_CODEC_UNSUPPORTED",
            Self::SeTrackNotfound => "SE_TRACK_NOTFOUND",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for SubscriptionError {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "SE_UNKNOWN",
            "SE_CODEC_UNSUPPORTED",
            "SE_TRACK_NOTFOUND",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscriptionError;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(SubscriptionError::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(SubscriptionError::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "SE_UNKNOWN" => Ok(SubscriptionError::SeUnknown),
                    "SE_CODEC_UNSUPPORTED" => Ok(SubscriptionError::SeCodecUnsupported),
                    "SE_TRACK_NOTFOUND" => Ok(SubscriptionError::SeTrackNotfound),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for SubscriptionPermission {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.all_participants {
            len += 1;
        }
        if !self.track_permissions.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SubscriptionPermission", len)?;
        if self.all_participants {
            struct_ser.serialize_field("allParticipants", &self.all_participants)?;
        }
        if !self.track_permissions.is_empty() {
            struct_ser.serialize_field("trackPermissions", &self.track_permissions)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscriptionPermission {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "all_participants",
            "allParticipants",
            "track_permissions",
            "trackPermissions",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AllParticipants,
            TrackPermissions,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "allParticipants" | "all_participants" => Ok(GeneratedField::AllParticipants),
                            "trackPermissions" | "track_permissions" => Ok(GeneratedField::TrackPermissions),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscriptionPermission;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SubscriptionPermission")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SubscriptionPermission, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut all_participants__ = None;
                let mut track_permissions__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::AllParticipants => {
                            if all_participants__.is_some() {
                                return Err(serde::de::Error::duplicate_field("allParticipants"));
                            }
                            all_participants__ = Some(map.next_value()?);
                        }
                        GeneratedField::TrackPermissions => {
                            if track_permissions__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackPermissions"));
                            }
                            track_permissions__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SubscriptionPermission {
                    all_participants: all_participants__.unwrap_or_default(),
                    track_permissions: track_permissions__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SubscriptionPermission", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubscriptionPermissionUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.participant_sid.is_empty() {
            len += 1;
        }
        if !self.track_sid.is_empty() {
            len += 1;
        }
        if self.allowed {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SubscriptionPermissionUpdate", len)?;
        if !self.participant_sid.is_empty() {
            struct_ser.serialize_field("participantSid", &self.participant_sid)?;
        }
        if !self.track_sid.is_empty() {
            struct_ser.serialize_field("trackSid", &self.track_sid)?;
        }
        if self.allowed {
            struct_ser.serialize_field("allowed", &self.allowed)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscriptionPermissionUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "participant_sid",
            "participantSid",
            "track_sid",
            "trackSid",
            "allowed",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ParticipantSid,
            TrackSid,
            Allowed,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "participantSid" | "participant_sid" => Ok(GeneratedField::ParticipantSid),
                            "trackSid" | "track_sid" => Ok(GeneratedField::TrackSid),
                            "allowed" => Ok(GeneratedField::Allowed),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscriptionPermissionUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SubscriptionPermissionUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SubscriptionPermissionUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut participant_sid__ = None;
                let mut track_sid__ = None;
                let mut allowed__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::ParticipantSid => {
                            if participant_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantSid"));
                            }
                            participant_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::TrackSid => {
                            if track_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSid"));
                            }
                            track_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Allowed => {
                            if allowed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("allowed"));
                            }
                            allowed__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SubscriptionPermissionUpdate {
                    participant_sid: participant_sid__.unwrap_or_default(),
                    track_sid: track_sid__.unwrap_or_default(),
                    allowed: allowed__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SubscriptionPermissionUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubscriptionResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.track_sid.is_empty() {
            len += 1;
        }
        if self.err != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SubscriptionResponse", len)?;
        if !self.track_sid.is_empty() {
            struct_ser.serialize_field("trackSid", &self.track_sid)?;
        }
        if self.err != 0 {
            let v = SubscriptionError::from_i32(self.err)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.err)))?;
            struct_ser.serialize_field("err", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscriptionResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "track_sid",
            "trackSid",
            "err",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TrackSid,
            Err,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "trackSid" | "track_sid" => Ok(GeneratedField::TrackSid),
                            "err" => Ok(GeneratedField::Err),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscriptionResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SubscriptionResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SubscriptionResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut track_sid__ = None;
                let mut err__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::TrackSid => {
                            if track_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSid"));
                            }
                            track_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Err => {
                            if err__.is_some() {
                                return Err(serde::de::Error::duplicate_field("err"));
                            }
                            err__ = Some(map.next_value::<SubscriptionError>()? as i32);
                        }
                    }
                }
                Ok(SubscriptionResponse {
                    track_sid: track_sid__.unwrap_or_default(),
                    err: err__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.SubscriptionResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SyncState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.answer.is_some() {
            len += 1;
        }
        if self.subscription.is_some() {
            len += 1;
        }
        if !self.publish_tracks.is_empty() {
            len += 1;
        }
        if !self.data_channels.is_empty() {
            len += 1;
        }
        if self.offer.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.SyncState", len)?;
        if let Some(v) = self.answer.as_ref() {
            struct_ser.serialize_field("answer", v)?;
        }
        if let Some(v) = self.subscription.as_ref() {
            struct_ser.serialize_field("subscription", v)?;
        }
        if !self.publish_tracks.is_empty() {
            struct_ser.serialize_field("publishTracks", &self.publish_tracks)?;
        }
        if !self.data_channels.is_empty() {
            struct_ser.serialize_field("dataChannels", &self.data_channels)?;
        }
        if let Some(v) = self.offer.as_ref() {
            struct_ser.serialize_field("offer", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SyncState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "answer",
            "subscription",
            "publish_tracks",
            "publishTracks",
            "data_channels",
            "dataChannels",
            "offer",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Answer,
            Subscription,
            PublishTracks,
            DataChannels,
            Offer,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "answer" => Ok(GeneratedField::Answer),
                            "subscription" => Ok(GeneratedField::Subscription),
                            "publishTracks" | "publish_tracks" => Ok(GeneratedField::PublishTracks),
                            "dataChannels" | "data_channels" => Ok(GeneratedField::DataChannels),
                            "offer" => Ok(GeneratedField::Offer),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SyncState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.SyncState")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SyncState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut answer__ = None;
                let mut subscription__ = None;
                let mut publish_tracks__ = None;
                let mut data_channels__ = None;
                let mut offer__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Answer => {
                            if answer__.is_some() {
                                return Err(serde::de::Error::duplicate_field("answer"));
                            }
                            answer__ = map.next_value()?;
                        }
                        GeneratedField::Subscription => {
                            if subscription__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscription"));
                            }
                            subscription__ = map.next_value()?;
                        }
                        GeneratedField::PublishTracks => {
                            if publish_tracks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("publishTracks"));
                            }
                            publish_tracks__ = Some(map.next_value()?);
                        }
                        GeneratedField::DataChannels => {
                            if data_channels__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dataChannels"));
                            }
                            data_channels__ = Some(map.next_value()?);
                        }
                        GeneratedField::Offer => {
                            if offer__.is_some() {
                                return Err(serde::de::Error::duplicate_field("offer"));
                            }
                            offer__ = map.next_value()?;
                        }
                    }
                }
                Ok(SyncState {
                    answer: answer__,
                    subscription: subscription__,
                    publish_tracks: publish_tracks__.unwrap_or_default(),
                    data_channels: data_channels__.unwrap_or_default(),
                    offer: offer__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.SyncState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TimedVersion {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.unix_micro != 0 {
            len += 1;
        }
        if self.ticks != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.TimedVersion", len)?;
        if self.unix_micro != 0 {
            struct_ser.serialize_field("unixMicro", ToString::to_string(&self.unix_micro).as_str())?;
        }
        if self.ticks != 0 {
            struct_ser.serialize_field("ticks", &self.ticks)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TimedVersion {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "unix_micro",
            "unixMicro",
            "ticks",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            UnixMicro,
            Ticks,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "unixMicro" | "unix_micro" => Ok(GeneratedField::UnixMicro),
                            "ticks" => Ok(GeneratedField::Ticks),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TimedVersion;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.TimedVersion")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<TimedVersion, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut unix_micro__ = None;
                let mut ticks__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::UnixMicro => {
                            if unix_micro__.is_some() {
                                return Err(serde::de::Error::duplicate_field("unixMicro"));
                            }
                            unix_micro__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Ticks => {
                            if ticks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ticks"));
                            }
                            ticks__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(TimedVersion {
                    unix_micro: unix_micro__.unwrap_or_default(),
                    ticks: ticks__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.TimedVersion", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TrackCompositeEgressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room_name.is_empty() {
            len += 1;
        }
        if !self.audio_track_id.is_empty() {
            len += 1;
        }
        if !self.video_track_id.is_empty() {
            len += 1;
        }
        if !self.file_outputs.is_empty() {
            len += 1;
        }
        if !self.stream_outputs.is_empty() {
            len += 1;
        }
        if !self.segment_outputs.is_empty() {
            len += 1;
        }
        if !self.image_outputs.is_empty() {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        if self.options.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.TrackCompositeEgressRequest", len)?;
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if !self.audio_track_id.is_empty() {
            struct_ser.serialize_field("audioTrackId", &self.audio_track_id)?;
        }
        if !self.video_track_id.is_empty() {
            struct_ser.serialize_field("videoTrackId", &self.video_track_id)?;
        }
        if !self.file_outputs.is_empty() {
            struct_ser.serialize_field("fileOutputs", &self.file_outputs)?;
        }
        if !self.stream_outputs.is_empty() {
            struct_ser.serialize_field("streamOutputs", &self.stream_outputs)?;
        }
        if !self.segment_outputs.is_empty() {
            struct_ser.serialize_field("segmentOutputs", &self.segment_outputs)?;
        }
        if !self.image_outputs.is_empty() {
            struct_ser.serialize_field("imageOutputs", &self.image_outputs)?;
        }
        if let Some(v) = self.output.as_ref() {
            match v {
                track_composite_egress_request::Output::File(v) => {
                    struct_ser.serialize_field("file", v)?;
                }
                track_composite_egress_request::Output::Stream(v) => {
                    struct_ser.serialize_field("stream", v)?;
                }
                track_composite_egress_request::Output::Segments(v) => {
                    struct_ser.serialize_field("segments", v)?;
                }
            }
        }
        if let Some(v) = self.options.as_ref() {
            match v {
                track_composite_egress_request::Options::Preset(v) => {
                    let v = EncodingOptionsPreset::from_i32(*v)
                        .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("preset", &v)?;
                }
                track_composite_egress_request::Options::Advanced(v) => {
                    struct_ser.serialize_field("advanced", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TrackCompositeEgressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room_name",
            "roomName",
            "audio_track_id",
            "audioTrackId",
            "video_track_id",
            "videoTrackId",
            "file_outputs",
            "fileOutputs",
            "stream_outputs",
            "streamOutputs",
            "segment_outputs",
            "segmentOutputs",
            "image_outputs",
            "imageOutputs",
            "file",
            "stream",
            "segments",
            "preset",
            "advanced",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RoomName,
            AudioTrackId,
            VideoTrackId,
            FileOutputs,
            StreamOutputs,
            SegmentOutputs,
            ImageOutputs,
            File,
            Stream,
            Segments,
            Preset,
            Advanced,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "audioTrackId" | "audio_track_id" => Ok(GeneratedField::AudioTrackId),
                            "videoTrackId" | "video_track_id" => Ok(GeneratedField::VideoTrackId),
                            "fileOutputs" | "file_outputs" => Ok(GeneratedField::FileOutputs),
                            "streamOutputs" | "stream_outputs" => Ok(GeneratedField::StreamOutputs),
                            "segmentOutputs" | "segment_outputs" => Ok(GeneratedField::SegmentOutputs),
                            "imageOutputs" | "image_outputs" => Ok(GeneratedField::ImageOutputs),
                            "file" => Ok(GeneratedField::File),
                            "stream" => Ok(GeneratedField::Stream),
                            "segments" => Ok(GeneratedField::Segments),
                            "preset" => Ok(GeneratedField::Preset),
                            "advanced" => Ok(GeneratedField::Advanced),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TrackCompositeEgressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.TrackCompositeEgressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<TrackCompositeEgressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room_name__ = None;
                let mut audio_track_id__ = None;
                let mut video_track_id__ = None;
                let mut file_outputs__ = None;
                let mut stream_outputs__ = None;
                let mut segment_outputs__ = None;
                let mut image_outputs__ = None;
                let mut output__ = None;
                let mut options__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::AudioTrackId => {
                            if audio_track_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioTrackId"));
                            }
                            audio_track_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::VideoTrackId => {
                            if video_track_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("videoTrackId"));
                            }
                            video_track_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::FileOutputs => {
                            if file_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fileOutputs"));
                            }
                            file_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::StreamOutputs => {
                            if stream_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("streamOutputs"));
                            }
                            stream_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::SegmentOutputs => {
                            if segment_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segmentOutputs"));
                            }
                            segment_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::ImageOutputs => {
                            if image_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("imageOutputs"));
                            }
                            image_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::File => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("file"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(track_composite_egress_request::Output::File)
;
                        }
                        GeneratedField::Stream => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stream"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(track_composite_egress_request::Output::Stream)
;
                        }
                        GeneratedField::Segments => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segments"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(track_composite_egress_request::Output::Segments)
;
                        }
                        GeneratedField::Preset => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preset"));
                            }
                            options__ = map.next_value::<::std::option::Option<EncodingOptionsPreset>>()?.map(|x| track_composite_egress_request::Options::Preset(x as i32));
                        }
                        GeneratedField::Advanced => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("advanced"));
                            }
                            options__ = map.next_value::<::std::option::Option<_>>()?.map(track_composite_egress_request::Options::Advanced)
;
                        }
                    }
                }
                Ok(TrackCompositeEgressRequest {
                    room_name: room_name__.unwrap_or_default(),
                    audio_track_id: audio_track_id__.unwrap_or_default(),
                    video_track_id: video_track_id__.unwrap_or_default(),
                    file_outputs: file_outputs__.unwrap_or_default(),
                    stream_outputs: stream_outputs__.unwrap_or_default(),
                    segment_outputs: segment_outputs__.unwrap_or_default(),
                    image_outputs: image_outputs__.unwrap_or_default(),
                    output: output__,
                    options: options__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.TrackCompositeEgressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TrackEgressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room_name.is_empty() {
            len += 1;
        }
        if !self.track_id.is_empty() {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.TrackEgressRequest", len)?;
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if !self.track_id.is_empty() {
            struct_ser.serialize_field("trackId", &self.track_id)?;
        }
        if let Some(v) = self.output.as_ref() {
            match v {
                track_egress_request::Output::File(v) => {
                    struct_ser.serialize_field("file", v)?;
                }
                track_egress_request::Output::WebsocketUrl(v) => {
                    struct_ser.serialize_field("websocketUrl", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TrackEgressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room_name",
            "roomName",
            "track_id",
            "trackId",
            "file",
            "websocket_url",
            "websocketUrl",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RoomName,
            TrackId,
            File,
            WebsocketUrl,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "trackId" | "track_id" => Ok(GeneratedField::TrackId),
                            "file" => Ok(GeneratedField::File),
                            "websocketUrl" | "websocket_url" => Ok(GeneratedField::WebsocketUrl),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TrackEgressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.TrackEgressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<TrackEgressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room_name__ = None;
                let mut track_id__ = None;
                let mut output__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::TrackId => {
                            if track_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackId"));
                            }
                            track_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::File => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("file"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(track_egress_request::Output::File)
;
                        }
                        GeneratedField::WebsocketUrl => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("websocketUrl"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(track_egress_request::Output::WebsocketUrl);
                        }
                    }
                }
                Ok(TrackEgressRequest {
                    room_name: room_name__.unwrap_or_default(),
                    track_id: track_id__.unwrap_or_default(),
                    output: output__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.TrackEgressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TrackInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.sid.is_empty() {
            len += 1;
        }
        if self.r#type != 0 {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        if self.muted {
            len += 1;
        }
        if self.width != 0 {
            len += 1;
        }
        if self.height != 0 {
            len += 1;
        }
        if self.simulcast {
            len += 1;
        }
        if self.disable_dtx {
            len += 1;
        }
        if self.source != 0 {
            len += 1;
        }
        if !self.layers.is_empty() {
            len += 1;
        }
        if !self.mime_type.is_empty() {
            len += 1;
        }
        if !self.mid.is_empty() {
            len += 1;
        }
        if !self.codecs.is_empty() {
            len += 1;
        }
        if self.stereo {
            len += 1;
        }
        if self.disable_red {
            len += 1;
        }
        if self.encryption != 0 {
            len += 1;
        }
        if !self.stream.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.TrackInfo", len)?;
        if !self.sid.is_empty() {
            struct_ser.serialize_field("sid", &self.sid)?;
        }
        if self.r#type != 0 {
            let v = TrackType::from_i32(self.r#type)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.r#type)))?;
            struct_ser.serialize_field("type", &v)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if self.muted {
            struct_ser.serialize_field("muted", &self.muted)?;
        }
        if self.width != 0 {
            struct_ser.serialize_field("width", &self.width)?;
        }
        if self.height != 0 {
            struct_ser.serialize_field("height", &self.height)?;
        }
        if self.simulcast {
            struct_ser.serialize_field("simulcast", &self.simulcast)?;
        }
        if self.disable_dtx {
            struct_ser.serialize_field("disableDtx", &self.disable_dtx)?;
        }
        if self.source != 0 {
            let v = TrackSource::from_i32(self.source)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.source)))?;
            struct_ser.serialize_field("source", &v)?;
        }
        if !self.layers.is_empty() {
            struct_ser.serialize_field("layers", &self.layers)?;
        }
        if !self.mime_type.is_empty() {
            struct_ser.serialize_field("mimeType", &self.mime_type)?;
        }
        if !self.mid.is_empty() {
            struct_ser.serialize_field("mid", &self.mid)?;
        }
        if !self.codecs.is_empty() {
            struct_ser.serialize_field("codecs", &self.codecs)?;
        }
        if self.stereo {
            struct_ser.serialize_field("stereo", &self.stereo)?;
        }
        if self.disable_red {
            struct_ser.serialize_field("disableRed", &self.disable_red)?;
        }
        if self.encryption != 0 {
            let v = encryption::Type::from_i32(self.encryption)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.encryption)))?;
            struct_ser.serialize_field("encryption", &v)?;
        }
        if !self.stream.is_empty() {
            struct_ser.serialize_field("stream", &self.stream)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TrackInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sid",
            "type",
            "name",
            "muted",
            "width",
            "height",
            "simulcast",
            "disable_dtx",
            "disableDtx",
            "source",
            "layers",
            "mime_type",
            "mimeType",
            "mid",
            "codecs",
            "stereo",
            "disable_red",
            "disableRed",
            "encryption",
            "stream",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Sid,
            Type,
            Name,
            Muted,
            Width,
            Height,
            Simulcast,
            DisableDtx,
            Source,
            Layers,
            MimeType,
            Mid,
            Codecs,
            Stereo,
            DisableRed,
            Encryption,
            Stream,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "sid" => Ok(GeneratedField::Sid),
                            "type" => Ok(GeneratedField::Type),
                            "name" => Ok(GeneratedField::Name),
                            "muted" => Ok(GeneratedField::Muted),
                            "width" => Ok(GeneratedField::Width),
                            "height" => Ok(GeneratedField::Height),
                            "simulcast" => Ok(GeneratedField::Simulcast),
                            "disableDtx" | "disable_dtx" => Ok(GeneratedField::DisableDtx),
                            "source" => Ok(GeneratedField::Source),
                            "layers" => Ok(GeneratedField::Layers),
                            "mimeType" | "mime_type" => Ok(GeneratedField::MimeType),
                            "mid" => Ok(GeneratedField::Mid),
                            "codecs" => Ok(GeneratedField::Codecs),
                            "stereo" => Ok(GeneratedField::Stereo),
                            "disableRed" | "disable_red" => Ok(GeneratedField::DisableRed),
                            "encryption" => Ok(GeneratedField::Encryption),
                            "stream" => Ok(GeneratedField::Stream),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TrackInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.TrackInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<TrackInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sid__ = None;
                let mut r#type__ = None;
                let mut name__ = None;
                let mut muted__ = None;
                let mut width__ = None;
                let mut height__ = None;
                let mut simulcast__ = None;
                let mut disable_dtx__ = None;
                let mut source__ = None;
                let mut layers__ = None;
                let mut mime_type__ = None;
                let mut mid__ = None;
                let mut codecs__ = None;
                let mut stereo__ = None;
                let mut disable_red__ = None;
                let mut encryption__ = None;
                let mut stream__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Sid => {
                            if sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sid"));
                            }
                            sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map.next_value::<TrackType>()? as i32);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::Muted => {
                            if muted__.is_some() {
                                return Err(serde::de::Error::duplicate_field("muted"));
                            }
                            muted__ = Some(map.next_value()?);
                        }
                        GeneratedField::Width => {
                            if width__.is_some() {
                                return Err(serde::de::Error::duplicate_field("width"));
                            }
                            width__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Simulcast => {
                            if simulcast__.is_some() {
                                return Err(serde::de::Error::duplicate_field("simulcast"));
                            }
                            simulcast__ = Some(map.next_value()?);
                        }
                        GeneratedField::DisableDtx => {
                            if disable_dtx__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableDtx"));
                            }
                            disable_dtx__ = Some(map.next_value()?);
                        }
                        GeneratedField::Source => {
                            if source__.is_some() {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source__ = Some(map.next_value::<TrackSource>()? as i32);
                        }
                        GeneratedField::Layers => {
                            if layers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("layers"));
                            }
                            layers__ = Some(map.next_value()?);
                        }
                        GeneratedField::MimeType => {
                            if mime_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mimeType"));
                            }
                            mime_type__ = Some(map.next_value()?);
                        }
                        GeneratedField::Mid => {
                            if mid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mid"));
                            }
                            mid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Codecs => {
                            if codecs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("codecs"));
                            }
                            codecs__ = Some(map.next_value()?);
                        }
                        GeneratedField::Stereo => {
                            if stereo__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stereo"));
                            }
                            stereo__ = Some(map.next_value()?);
                        }
                        GeneratedField::DisableRed => {
                            if disable_red__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disableRed"));
                            }
                            disable_red__ = Some(map.next_value()?);
                        }
                        GeneratedField::Encryption => {
                            if encryption__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encryption"));
                            }
                            encryption__ = Some(map.next_value::<encryption::Type>()? as i32);
                        }
                        GeneratedField::Stream => {
                            if stream__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stream"));
                            }
                            stream__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(TrackInfo {
                    sid: sid__.unwrap_or_default(),
                    r#type: r#type__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                    muted: muted__.unwrap_or_default(),
                    width: width__.unwrap_or_default(),
                    height: height__.unwrap_or_default(),
                    simulcast: simulcast__.unwrap_or_default(),
                    disable_dtx: disable_dtx__.unwrap_or_default(),
                    source: source__.unwrap_or_default(),
                    layers: layers__.unwrap_or_default(),
                    mime_type: mime_type__.unwrap_or_default(),
                    mid: mid__.unwrap_or_default(),
                    codecs: codecs__.unwrap_or_default(),
                    stereo: stereo__.unwrap_or_default(),
                    disable_red: disable_red__.unwrap_or_default(),
                    encryption: encryption__.unwrap_or_default(),
                    stream: stream__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.TrackInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TrackPermission {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.participant_sid.is_empty() {
            len += 1;
        }
        if self.all_tracks {
            len += 1;
        }
        if !self.track_sids.is_empty() {
            len += 1;
        }
        if !self.participant_identity.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.TrackPermission", len)?;
        if !self.participant_sid.is_empty() {
            struct_ser.serialize_field("participantSid", &self.participant_sid)?;
        }
        if self.all_tracks {
            struct_ser.serialize_field("allTracks", &self.all_tracks)?;
        }
        if !self.track_sids.is_empty() {
            struct_ser.serialize_field("trackSids", &self.track_sids)?;
        }
        if !self.participant_identity.is_empty() {
            struct_ser.serialize_field("participantIdentity", &self.participant_identity)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TrackPermission {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "participant_sid",
            "participantSid",
            "all_tracks",
            "allTracks",
            "track_sids",
            "trackSids",
            "participant_identity",
            "participantIdentity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ParticipantSid,
            AllTracks,
            TrackSids,
            ParticipantIdentity,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "participantSid" | "participant_sid" => Ok(GeneratedField::ParticipantSid),
                            "allTracks" | "all_tracks" => Ok(GeneratedField::AllTracks),
                            "trackSids" | "track_sids" => Ok(GeneratedField::TrackSids),
                            "participantIdentity" | "participant_identity" => Ok(GeneratedField::ParticipantIdentity),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TrackPermission;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.TrackPermission")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<TrackPermission, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut participant_sid__ = None;
                let mut all_tracks__ = None;
                let mut track_sids__ = None;
                let mut participant_identity__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::ParticipantSid => {
                            if participant_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantSid"));
                            }
                            participant_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::AllTracks => {
                            if all_tracks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("allTracks"));
                            }
                            all_tracks__ = Some(map.next_value()?);
                        }
                        GeneratedField::TrackSids => {
                            if track_sids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSids"));
                            }
                            track_sids__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantIdentity => {
                            if participant_identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantIdentity"));
                            }
                            participant_identity__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(TrackPermission {
                    participant_sid: participant_sid__.unwrap_or_default(),
                    all_tracks: all_tracks__.unwrap_or_default(),
                    track_sids: track_sids__.unwrap_or_default(),
                    participant_identity: participant_identity__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.TrackPermission", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TrackPublishedResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.cid.is_empty() {
            len += 1;
        }
        if self.track.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.TrackPublishedResponse", len)?;
        if !self.cid.is_empty() {
            struct_ser.serialize_field("cid", &self.cid)?;
        }
        if let Some(v) = self.track.as_ref() {
            struct_ser.serialize_field("track", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TrackPublishedResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "cid",
            "track",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Cid,
            Track,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "cid" => Ok(GeneratedField::Cid),
                            "track" => Ok(GeneratedField::Track),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TrackPublishedResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.TrackPublishedResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<TrackPublishedResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut cid__ = None;
                let mut track__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Cid => {
                            if cid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("cid"));
                            }
                            cid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Track => {
                            if track__.is_some() {
                                return Err(serde::de::Error::duplicate_field("track"));
                            }
                            track__ = map.next_value()?;
                        }
                    }
                }
                Ok(TrackPublishedResponse {
                    cid: cid__.unwrap_or_default(),
                    track: track__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.TrackPublishedResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TrackSource {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unknown => "UNKNOWN",
            Self::Camera => "CAMERA",
            Self::Microphone => "MICROPHONE",
            Self::ScreenShare => "SCREEN_SHARE",
            Self::ScreenShareAudio => "SCREEN_SHARE_AUDIO",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for TrackSource {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "UNKNOWN",
            "CAMERA",
            "MICROPHONE",
            "SCREEN_SHARE",
            "SCREEN_SHARE_AUDIO",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TrackSource;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(TrackSource::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(TrackSource::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "UNKNOWN" => Ok(TrackSource::Unknown),
                    "CAMERA" => Ok(TrackSource::Camera),
                    "MICROPHONE" => Ok(TrackSource::Microphone),
                    "SCREEN_SHARE" => Ok(TrackSource::ScreenShare),
                    "SCREEN_SHARE_AUDIO" => Ok(TrackSource::ScreenShareAudio),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for TrackType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Audio => "AUDIO",
            Self::Video => "VIDEO",
            Self::Data => "DATA",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for TrackType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "AUDIO",
            "VIDEO",
            "DATA",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TrackType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(TrackType::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(TrackType::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "AUDIO" => Ok(TrackType::Audio),
                    "VIDEO" => Ok(TrackType::Video),
                    "DATA" => Ok(TrackType::Data),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for TrackUnpublishedResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.track_sid.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.TrackUnpublishedResponse", len)?;
        if !self.track_sid.is_empty() {
            struct_ser.serialize_field("trackSid", &self.track_sid)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TrackUnpublishedResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "track_sid",
            "trackSid",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TrackSid,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "trackSid" | "track_sid" => Ok(GeneratedField::TrackSid),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TrackUnpublishedResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.TrackUnpublishedResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<TrackUnpublishedResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut track_sid__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::TrackSid => {
                            if track_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSid"));
                            }
                            track_sid__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(TrackUnpublishedResponse {
                    track_sid: track_sid__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.TrackUnpublishedResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TrickleRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.candidate_init.is_empty() {
            len += 1;
        }
        if self.target != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.TrickleRequest", len)?;
        if !self.candidate_init.is_empty() {
            struct_ser.serialize_field("candidateInit", &self.candidate_init)?;
        }
        if self.target != 0 {
            let v = SignalTarget::from_i32(self.target)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.target)))?;
            struct_ser.serialize_field("target", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TrickleRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "candidateInit",
            "target",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CandidateInit,
            Target,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "candidateInit" => Ok(GeneratedField::CandidateInit),
                            "target" => Ok(GeneratedField::Target),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TrickleRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.TrickleRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<TrickleRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut candidate_init__ = None;
                let mut target__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::CandidateInit => {
                            if candidate_init__.is_some() {
                                return Err(serde::de::Error::duplicate_field("candidateInit"));
                            }
                            candidate_init__ = Some(map.next_value()?);
                        }
                        GeneratedField::Target => {
                            if target__.is_some() {
                                return Err(serde::de::Error::duplicate_field("target"));
                            }
                            target__ = Some(map.next_value::<SignalTarget>()? as i32);
                        }
                    }
                }
                Ok(TrickleRequest {
                    candidate_init: candidate_init__.unwrap_or_default(),
                    target: target__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.TrickleRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateIngressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.ingress_id.is_empty() {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        if !self.room_name.is_empty() {
            len += 1;
        }
        if !self.participant_identity.is_empty() {
            len += 1;
        }
        if !self.participant_name.is_empty() {
            len += 1;
        }
        if self.bypass_transcoding.is_some() {
            len += 1;
        }
        if self.audio.is_some() {
            len += 1;
        }
        if self.video.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateIngressRequest", len)?;
        if !self.ingress_id.is_empty() {
            struct_ser.serialize_field("ingressId", &self.ingress_id)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if !self.room_name.is_empty() {
            struct_ser.serialize_field("roomName", &self.room_name)?;
        }
        if !self.participant_identity.is_empty() {
            struct_ser.serialize_field("participantIdentity", &self.participant_identity)?;
        }
        if !self.participant_name.is_empty() {
            struct_ser.serialize_field("participantName", &self.participant_name)?;
        }
        if let Some(v) = self.bypass_transcoding.as_ref() {
            struct_ser.serialize_field("bypassTranscoding", v)?;
        }
        if let Some(v) = self.audio.as_ref() {
            struct_ser.serialize_field("audio", v)?;
        }
        if let Some(v) = self.video.as_ref() {
            struct_ser.serialize_field("video", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateIngressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ingress_id",
            "ingressId",
            "name",
            "room_name",
            "roomName",
            "participant_identity",
            "participantIdentity",
            "participant_name",
            "participantName",
            "bypass_transcoding",
            "bypassTranscoding",
            "audio",
            "video",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IngressId,
            Name,
            RoomName,
            ParticipantIdentity,
            ParticipantName,
            BypassTranscoding,
            Audio,
            Video,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "ingressId" | "ingress_id" => Ok(GeneratedField::IngressId),
                            "name" => Ok(GeneratedField::Name),
                            "roomName" | "room_name" => Ok(GeneratedField::RoomName),
                            "participantIdentity" | "participant_identity" => Ok(GeneratedField::ParticipantIdentity),
                            "participantName" | "participant_name" => Ok(GeneratedField::ParticipantName),
                            "bypassTranscoding" | "bypass_transcoding" => Ok(GeneratedField::BypassTranscoding),
                            "audio" => Ok(GeneratedField::Audio),
                            "video" => Ok(GeneratedField::Video),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateIngressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateIngressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateIngressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut ingress_id__ = None;
                let mut name__ = None;
                let mut room_name__ = None;
                let mut participant_identity__ = None;
                let mut participant_name__ = None;
                let mut bypass_transcoding__ = None;
                let mut audio__ = None;
                let mut video__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IngressId => {
                            if ingress_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ingressId"));
                            }
                            ingress_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                        GeneratedField::RoomName => {
                            if room_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("roomName"));
                            }
                            room_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantIdentity => {
                            if participant_identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantIdentity"));
                            }
                            participant_identity__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantName => {
                            if participant_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantName"));
                            }
                            participant_name__ = Some(map.next_value()?);
                        }
                        GeneratedField::BypassTranscoding => {
                            if bypass_transcoding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bypassTranscoding"));
                            }
                            bypass_transcoding__ = map.next_value()?;
                        }
                        GeneratedField::Audio => {
                            if audio__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audio"));
                            }
                            audio__ = map.next_value()?;
                        }
                        GeneratedField::Video => {
                            if video__.is_some() {
                                return Err(serde::de::Error::duplicate_field("video"));
                            }
                            video__ = map.next_value()?;
                        }
                    }
                }
                Ok(UpdateIngressRequest {
                    ingress_id: ingress_id__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                    room_name: room_name__.unwrap_or_default(),
                    participant_identity: participant_identity__.unwrap_or_default(),
                    participant_name: participant_name__.unwrap_or_default(),
                    bypass_transcoding: bypass_transcoding__,
                    audio: audio__,
                    video: video__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateIngressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateLayoutRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.egress_id.is_empty() {
            len += 1;
        }
        if !self.layout.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateLayoutRequest", len)?;
        if !self.egress_id.is_empty() {
            struct_ser.serialize_field("egressId", &self.egress_id)?;
        }
        if !self.layout.is_empty() {
            struct_ser.serialize_field("layout", &self.layout)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateLayoutRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "egress_id",
            "egressId",
            "layout",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EgressId,
            Layout,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "egressId" | "egress_id" => Ok(GeneratedField::EgressId),
                            "layout" => Ok(GeneratedField::Layout),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateLayoutRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateLayoutRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateLayoutRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut egress_id__ = None;
                let mut layout__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::EgressId => {
                            if egress_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("egressId"));
                            }
                            egress_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::Layout => {
                            if layout__.is_some() {
                                return Err(serde::de::Error::duplicate_field("layout"));
                            }
                            layout__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(UpdateLayoutRequest {
                    egress_id: egress_id__.unwrap_or_default(),
                    layout: layout__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateLayoutRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateParticipantMetadata {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.metadata.is_empty() {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateParticipantMetadata", len)?;
        if !self.metadata.is_empty() {
            struct_ser.serialize_field("metadata", &self.metadata)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateParticipantMetadata {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "metadata",
            "name",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Metadata,
            Name,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "metadata" => Ok(GeneratedField::Metadata),
                            "name" => Ok(GeneratedField::Name),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateParticipantMetadata;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateParticipantMetadata")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateParticipantMetadata, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut metadata__ = None;
                let mut name__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = Some(map.next_value()?);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(UpdateParticipantMetadata {
                    metadata: metadata__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateParticipantMetadata", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateParticipantRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room.is_empty() {
            len += 1;
        }
        if !self.identity.is_empty() {
            len += 1;
        }
        if !self.metadata.is_empty() {
            len += 1;
        }
        if self.permission.is_some() {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateParticipantRequest", len)?;
        if !self.room.is_empty() {
            struct_ser.serialize_field("room", &self.room)?;
        }
        if !self.identity.is_empty() {
            struct_ser.serialize_field("identity", &self.identity)?;
        }
        if !self.metadata.is_empty() {
            struct_ser.serialize_field("metadata", &self.metadata)?;
        }
        if let Some(v) = self.permission.as_ref() {
            struct_ser.serialize_field("permission", v)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateParticipantRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
            "identity",
            "metadata",
            "permission",
            "name",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
            Identity,
            Metadata,
            Permission,
            Name,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            "identity" => Ok(GeneratedField::Identity),
                            "metadata" => Ok(GeneratedField::Metadata),
                            "permission" => Ok(GeneratedField::Permission),
                            "name" => Ok(GeneratedField::Name),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateParticipantRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateParticipantRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateParticipantRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                let mut identity__ = None;
                let mut metadata__ = None;
                let mut permission__ = None;
                let mut name__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = Some(map.next_value()?);
                        }
                        GeneratedField::Identity => {
                            if identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identity"));
                            }
                            identity__ = Some(map.next_value()?);
                        }
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = Some(map.next_value()?);
                        }
                        GeneratedField::Permission => {
                            if permission__.is_some() {
                                return Err(serde::de::Error::duplicate_field("permission"));
                            }
                            permission__ = map.next_value()?;
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(UpdateParticipantRequest {
                    room: room__.unwrap_or_default(),
                    identity: identity__.unwrap_or_default(),
                    metadata: metadata__.unwrap_or_default(),
                    permission: permission__,
                    name: name__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateParticipantRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateRoomMetadataRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room.is_empty() {
            len += 1;
        }
        if !self.metadata.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateRoomMetadataRequest", len)?;
        if !self.room.is_empty() {
            struct_ser.serialize_field("room", &self.room)?;
        }
        if !self.metadata.is_empty() {
            struct_ser.serialize_field("metadata", &self.metadata)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateRoomMetadataRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
            "metadata",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
            Metadata,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            "metadata" => Ok(GeneratedField::Metadata),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateRoomMetadataRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateRoomMetadataRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateRoomMetadataRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                let mut metadata__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = Some(map.next_value()?);
                        }
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(UpdateRoomMetadataRequest {
                    room: room__.unwrap_or_default(),
                    metadata: metadata__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateRoomMetadataRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateStreamRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.egress_id.is_empty() {
            len += 1;
        }
        if !self.add_output_urls.is_empty() {
            len += 1;
        }
        if !self.remove_output_urls.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateStreamRequest", len)?;
        if !self.egress_id.is_empty() {
            struct_ser.serialize_field("egressId", &self.egress_id)?;
        }
        if !self.add_output_urls.is_empty() {
            struct_ser.serialize_field("addOutputUrls", &self.add_output_urls)?;
        }
        if !self.remove_output_urls.is_empty() {
            struct_ser.serialize_field("removeOutputUrls", &self.remove_output_urls)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateStreamRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "egress_id",
            "egressId",
            "add_output_urls",
            "addOutputUrls",
            "remove_output_urls",
            "removeOutputUrls",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EgressId,
            AddOutputUrls,
            RemoveOutputUrls,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "egressId" | "egress_id" => Ok(GeneratedField::EgressId),
                            "addOutputUrls" | "add_output_urls" => Ok(GeneratedField::AddOutputUrls),
                            "removeOutputUrls" | "remove_output_urls" => Ok(GeneratedField::RemoveOutputUrls),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateStreamRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateStreamRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateStreamRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut egress_id__ = None;
                let mut add_output_urls__ = None;
                let mut remove_output_urls__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::EgressId => {
                            if egress_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("egressId"));
                            }
                            egress_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::AddOutputUrls => {
                            if add_output_urls__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addOutputUrls"));
                            }
                            add_output_urls__ = Some(map.next_value()?);
                        }
                        GeneratedField::RemoveOutputUrls => {
                            if remove_output_urls__.is_some() {
                                return Err(serde::de::Error::duplicate_field("removeOutputUrls"));
                            }
                            remove_output_urls__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(UpdateStreamRequest {
                    egress_id: egress_id__.unwrap_or_default(),
                    add_output_urls: add_output_urls__.unwrap_or_default(),
                    remove_output_urls: remove_output_urls__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateStreamRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateSubscription {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.track_sids.is_empty() {
            len += 1;
        }
        if self.subscribe {
            len += 1;
        }
        if !self.participant_tracks.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateSubscription", len)?;
        if !self.track_sids.is_empty() {
            struct_ser.serialize_field("trackSids", &self.track_sids)?;
        }
        if self.subscribe {
            struct_ser.serialize_field("subscribe", &self.subscribe)?;
        }
        if !self.participant_tracks.is_empty() {
            struct_ser.serialize_field("participantTracks", &self.participant_tracks)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateSubscription {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "track_sids",
            "trackSids",
            "subscribe",
            "participant_tracks",
            "participantTracks",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TrackSids,
            Subscribe,
            ParticipantTracks,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "trackSids" | "track_sids" => Ok(GeneratedField::TrackSids),
                            "subscribe" => Ok(GeneratedField::Subscribe),
                            "participantTracks" | "participant_tracks" => Ok(GeneratedField::ParticipantTracks),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateSubscription;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateSubscription")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateSubscription, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut track_sids__ = None;
                let mut subscribe__ = None;
                let mut participant_tracks__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::TrackSids => {
                            if track_sids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSids"));
                            }
                            track_sids__ = Some(map.next_value()?);
                        }
                        GeneratedField::Subscribe => {
                            if subscribe__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscribe"));
                            }
                            subscribe__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantTracks => {
                            if participant_tracks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantTracks"));
                            }
                            participant_tracks__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(UpdateSubscription {
                    track_sids: track_sids__.unwrap_or_default(),
                    subscribe: subscribe__.unwrap_or_default(),
                    participant_tracks: participant_tracks__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateSubscription", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateSubscriptionsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.room.is_empty() {
            len += 1;
        }
        if !self.identity.is_empty() {
            len += 1;
        }
        if !self.track_sids.is_empty() {
            len += 1;
        }
        if self.subscribe {
            len += 1;
        }
        if !self.participant_tracks.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateSubscriptionsRequest", len)?;
        if !self.room.is_empty() {
            struct_ser.serialize_field("room", &self.room)?;
        }
        if !self.identity.is_empty() {
            struct_ser.serialize_field("identity", &self.identity)?;
        }
        if !self.track_sids.is_empty() {
            struct_ser.serialize_field("trackSids", &self.track_sids)?;
        }
        if self.subscribe {
            struct_ser.serialize_field("subscribe", &self.subscribe)?;
        }
        if !self.participant_tracks.is_empty() {
            struct_ser.serialize_field("participantTracks", &self.participant_tracks)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateSubscriptionsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "room",
            "identity",
            "track_sids",
            "trackSids",
            "subscribe",
            "participant_tracks",
            "participantTracks",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Room,
            Identity,
            TrackSids,
            Subscribe,
            ParticipantTracks,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "room" => Ok(GeneratedField::Room),
                            "identity" => Ok(GeneratedField::Identity),
                            "trackSids" | "track_sids" => Ok(GeneratedField::TrackSids),
                            "subscribe" => Ok(GeneratedField::Subscribe),
                            "participantTracks" | "participant_tracks" => Ok(GeneratedField::ParticipantTracks),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateSubscriptionsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateSubscriptionsRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateSubscriptionsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut room__ = None;
                let mut identity__ = None;
                let mut track_sids__ = None;
                let mut subscribe__ = None;
                let mut participant_tracks__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = Some(map.next_value()?);
                        }
                        GeneratedField::Identity => {
                            if identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identity"));
                            }
                            identity__ = Some(map.next_value()?);
                        }
                        GeneratedField::TrackSids => {
                            if track_sids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSids"));
                            }
                            track_sids__ = Some(map.next_value()?);
                        }
                        GeneratedField::Subscribe => {
                            if subscribe__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscribe"));
                            }
                            subscribe__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantTracks => {
                            if participant_tracks__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantTracks"));
                            }
                            participant_tracks__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(UpdateSubscriptionsRequest {
                    room: room__.unwrap_or_default(),
                    identity: identity__.unwrap_or_default(),
                    track_sids: track_sids__.unwrap_or_default(),
                    subscribe: subscribe__.unwrap_or_default(),
                    participant_tracks: participant_tracks__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateSubscriptionsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateSubscriptionsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.UpdateSubscriptionsResponse", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateSubscriptionsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Err(serde::de::Error::unknown_field(value, FIELDS))
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateSubscriptionsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateSubscriptionsResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateSubscriptionsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map.next_key::<GeneratedField>()?.is_some() {
                    let _ = map.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(UpdateSubscriptionsResponse {
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateSubscriptionsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateTrackSettings {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.track_sids.is_empty() {
            len += 1;
        }
        if self.disabled {
            len += 1;
        }
        if self.quality != 0 {
            len += 1;
        }
        if self.width != 0 {
            len += 1;
        }
        if self.height != 0 {
            len += 1;
        }
        if self.fps != 0 {
            len += 1;
        }
        if self.priority != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateTrackSettings", len)?;
        if !self.track_sids.is_empty() {
            struct_ser.serialize_field("trackSids", &self.track_sids)?;
        }
        if self.disabled {
            struct_ser.serialize_field("disabled", &self.disabled)?;
        }
        if self.quality != 0 {
            let v = VideoQuality::from_i32(self.quality)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.quality)))?;
            struct_ser.serialize_field("quality", &v)?;
        }
        if self.width != 0 {
            struct_ser.serialize_field("width", &self.width)?;
        }
        if self.height != 0 {
            struct_ser.serialize_field("height", &self.height)?;
        }
        if self.fps != 0 {
            struct_ser.serialize_field("fps", &self.fps)?;
        }
        if self.priority != 0 {
            struct_ser.serialize_field("priority", &self.priority)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateTrackSettings {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "track_sids",
            "trackSids",
            "disabled",
            "quality",
            "width",
            "height",
            "fps",
            "priority",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TrackSids,
            Disabled,
            Quality,
            Width,
            Height,
            Fps,
            Priority,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "trackSids" | "track_sids" => Ok(GeneratedField::TrackSids),
                            "disabled" => Ok(GeneratedField::Disabled),
                            "quality" => Ok(GeneratedField::Quality),
                            "width" => Ok(GeneratedField::Width),
                            "height" => Ok(GeneratedField::Height),
                            "fps" => Ok(GeneratedField::Fps),
                            "priority" => Ok(GeneratedField::Priority),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateTrackSettings;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateTrackSettings")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateTrackSettings, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut track_sids__ = None;
                let mut disabled__ = None;
                let mut quality__ = None;
                let mut width__ = None;
                let mut height__ = None;
                let mut fps__ = None;
                let mut priority__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::TrackSids => {
                            if track_sids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSids"));
                            }
                            track_sids__ = Some(map.next_value()?);
                        }
                        GeneratedField::Disabled => {
                            if disabled__.is_some() {
                                return Err(serde::de::Error::duplicate_field("disabled"));
                            }
                            disabled__ = Some(map.next_value()?);
                        }
                        GeneratedField::Quality => {
                            if quality__.is_some() {
                                return Err(serde::de::Error::duplicate_field("quality"));
                            }
                            quality__ = Some(map.next_value::<VideoQuality>()? as i32);
                        }
                        GeneratedField::Width => {
                            if width__.is_some() {
                                return Err(serde::de::Error::duplicate_field("width"));
                            }
                            width__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Fps => {
                            if fps__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fps"));
                            }
                            fps__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Priority => {
                            if priority__.is_some() {
                                return Err(serde::de::Error::duplicate_field("priority"));
                            }
                            priority__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(UpdateTrackSettings {
                    track_sids: track_sids__.unwrap_or_default(),
                    disabled: disabled__.unwrap_or_default(),
                    quality: quality__.unwrap_or_default(),
                    width: width__.unwrap_or_default(),
                    height: height__.unwrap_or_default(),
                    fps: fps__.unwrap_or_default(),
                    priority: priority__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateTrackSettings", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateVideoLayers {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.track_sid.is_empty() {
            len += 1;
        }
        if !self.layers.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UpdateVideoLayers", len)?;
        if !self.track_sid.is_empty() {
            struct_ser.serialize_field("trackSid", &self.track_sid)?;
        }
        if !self.layers.is_empty() {
            struct_ser.serialize_field("layers", &self.layers)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateVideoLayers {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "track_sid",
            "trackSid",
            "layers",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TrackSid,
            Layers,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "trackSid" | "track_sid" => Ok(GeneratedField::TrackSid),
                            "layers" => Ok(GeneratedField::Layers),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdateVideoLayers;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UpdateVideoLayers")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UpdateVideoLayers, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut track_sid__ = None;
                let mut layers__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::TrackSid => {
                            if track_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("trackSid"));
                            }
                            track_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::Layers => {
                            if layers__.is_some() {
                                return Err(serde::de::Error::duplicate_field("layers"));
                            }
                            layers__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(UpdateVideoLayers {
                    track_sid: track_sid__.unwrap_or_default(),
                    layers: layers__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.UpdateVideoLayers", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UserPacket {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.participant_sid.is_empty() {
            len += 1;
        }
        if !self.participant_identity.is_empty() {
            len += 1;
        }
        if !self.payload.is_empty() {
            len += 1;
        }
        if !self.destination_sids.is_empty() {
            len += 1;
        }
        if !self.destination_identities.is_empty() {
            len += 1;
        }
        if self.topic.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.UserPacket", len)?;
        if !self.participant_sid.is_empty() {
            struct_ser.serialize_field("participantSid", &self.participant_sid)?;
        }
        if !self.participant_identity.is_empty() {
            struct_ser.serialize_field("participantIdentity", &self.participant_identity)?;
        }
        if !self.payload.is_empty() {
            struct_ser.serialize_field("payload", pbjson::private::base64::encode(&self.payload).as_str())?;
        }
        if !self.destination_sids.is_empty() {
            struct_ser.serialize_field("destinationSids", &self.destination_sids)?;
        }
        if !self.destination_identities.is_empty() {
            struct_ser.serialize_field("destinationIdentities", &self.destination_identities)?;
        }
        if let Some(v) = self.topic.as_ref() {
            struct_ser.serialize_field("topic", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UserPacket {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "participant_sid",
            "participantSid",
            "participant_identity",
            "participantIdentity",
            "payload",
            "destination_sids",
            "destinationSids",
            "destination_identities",
            "destinationIdentities",
            "topic",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ParticipantSid,
            ParticipantIdentity,
            Payload,
            DestinationSids,
            DestinationIdentities,
            Topic,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "participantSid" | "participant_sid" => Ok(GeneratedField::ParticipantSid),
                            "participantIdentity" | "participant_identity" => Ok(GeneratedField::ParticipantIdentity),
                            "payload" => Ok(GeneratedField::Payload),
                            "destinationSids" | "destination_sids" => Ok(GeneratedField::DestinationSids),
                            "destinationIdentities" | "destination_identities" => Ok(GeneratedField::DestinationIdentities),
                            "topic" => Ok(GeneratedField::Topic),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UserPacket;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.UserPacket")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UserPacket, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut participant_sid__ = None;
                let mut participant_identity__ = None;
                let mut payload__ = None;
                let mut destination_sids__ = None;
                let mut destination_identities__ = None;
                let mut topic__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::ParticipantSid => {
                            if participant_sid__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantSid"));
                            }
                            participant_sid__ = Some(map.next_value()?);
                        }
                        GeneratedField::ParticipantIdentity => {
                            if participant_identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participantIdentity"));
                            }
                            participant_identity__ = Some(map.next_value()?);
                        }
                        GeneratedField::Payload => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payload"));
                            }
                            payload__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DestinationSids => {
                            if destination_sids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("destinationSids"));
                            }
                            destination_sids__ = Some(map.next_value()?);
                        }
                        GeneratedField::DestinationIdentities => {
                            if destination_identities__.is_some() {
                                return Err(serde::de::Error::duplicate_field("destinationIdentities"));
                            }
                            destination_identities__ = Some(map.next_value()?);
                        }
                        GeneratedField::Topic => {
                            if topic__.is_some() {
                                return Err(serde::de::Error::duplicate_field("topic"));
                            }
                            topic__ = map.next_value()?;
                        }
                    }
                }
                Ok(UserPacket {
                    participant_sid: participant_sid__.unwrap_or_default(),
                    participant_identity: participant_identity__.unwrap_or_default(),
                    payload: payload__.unwrap_or_default(),
                    destination_sids: destination_sids__.unwrap_or_default(),
                    destination_identities: destination_identities__.unwrap_or_default(),
                    topic: topic__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.UserPacket", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VideoCodec {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::DefaultVc => "DEFAULT_VC",
            Self::H264Baseline => "H264_BASELINE",
            Self::H264Main => "H264_MAIN",
            Self::H264High => "H264_HIGH",
            Self::Vp8 => "VP8",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for VideoCodec {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "DEFAULT_VC",
            "H264_BASELINE",
            "H264_MAIN",
            "H264_HIGH",
            "VP8",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VideoCodec;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(VideoCodec::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(VideoCodec::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "DEFAULT_VC" => Ok(VideoCodec::DefaultVc),
                    "H264_BASELINE" => Ok(VideoCodec::H264Baseline),
                    "H264_MAIN" => Ok(VideoCodec::H264Main),
                    "H264_HIGH" => Ok(VideoCodec::H264High),
                    "VP8" => Ok(VideoCodec::Vp8),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for VideoConfiguration {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.hardware_encoder != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.VideoConfiguration", len)?;
        if self.hardware_encoder != 0 {
            let v = ClientConfigSetting::from_i32(self.hardware_encoder)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.hardware_encoder)))?;
            struct_ser.serialize_field("hardwareEncoder", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VideoConfiguration {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "hardware_encoder",
            "hardwareEncoder",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            HardwareEncoder,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "hardwareEncoder" | "hardware_encoder" => Ok(GeneratedField::HardwareEncoder),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VideoConfiguration;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.VideoConfiguration")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<VideoConfiguration, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut hardware_encoder__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::HardwareEncoder => {
                            if hardware_encoder__.is_some() {
                                return Err(serde::de::Error::duplicate_field("hardwareEncoder"));
                            }
                            hardware_encoder__ = Some(map.next_value::<ClientConfigSetting>()? as i32);
                        }
                    }
                }
                Ok(VideoConfiguration {
                    hardware_encoder: hardware_encoder__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.VideoConfiguration", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VideoLayer {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.quality != 0 {
            len += 1;
        }
        if self.width != 0 {
            len += 1;
        }
        if self.height != 0 {
            len += 1;
        }
        if self.bitrate != 0 {
            len += 1;
        }
        if self.ssrc != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.VideoLayer", len)?;
        if self.quality != 0 {
            let v = VideoQuality::from_i32(self.quality)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.quality)))?;
            struct_ser.serialize_field("quality", &v)?;
        }
        if self.width != 0 {
            struct_ser.serialize_field("width", &self.width)?;
        }
        if self.height != 0 {
            struct_ser.serialize_field("height", &self.height)?;
        }
        if self.bitrate != 0 {
            struct_ser.serialize_field("bitrate", &self.bitrate)?;
        }
        if self.ssrc != 0 {
            struct_ser.serialize_field("ssrc", &self.ssrc)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VideoLayer {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "quality",
            "width",
            "height",
            "bitrate",
            "ssrc",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Quality,
            Width,
            Height,
            Bitrate,
            Ssrc,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "quality" => Ok(GeneratedField::Quality),
                            "width" => Ok(GeneratedField::Width),
                            "height" => Ok(GeneratedField::Height),
                            "bitrate" => Ok(GeneratedField::Bitrate),
                            "ssrc" => Ok(GeneratedField::Ssrc),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VideoLayer;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.VideoLayer")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<VideoLayer, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut quality__ = None;
                let mut width__ = None;
                let mut height__ = None;
                let mut bitrate__ = None;
                let mut ssrc__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Quality => {
                            if quality__.is_some() {
                                return Err(serde::de::Error::duplicate_field("quality"));
                            }
                            quality__ = Some(map.next_value::<VideoQuality>()? as i32);
                        }
                        GeneratedField::Width => {
                            if width__.is_some() {
                                return Err(serde::de::Error::duplicate_field("width"));
                            }
                            width__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Height => {
                            if height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("height"));
                            }
                            height__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Bitrate => {
                            if bitrate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bitrate"));
                            }
                            bitrate__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Ssrc => {
                            if ssrc__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ssrc"));
                            }
                            ssrc__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(VideoLayer {
                    quality: quality__.unwrap_or_default(),
                    width: width__.unwrap_or_default(),
                    height: height__.unwrap_or_default(),
                    bitrate: bitrate__.unwrap_or_default(),
                    ssrc: ssrc__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.VideoLayer", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VideoQuality {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Off => "OFF",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for VideoQuality {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "LOW",
            "MEDIUM",
            "HIGH",
            "OFF",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VideoQuality;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(VideoQuality::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(VideoQuality::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "LOW" => Ok(VideoQuality::Low),
                    "MEDIUM" => Ok(VideoQuality::Medium),
                    "HIGH" => Ok(VideoQuality::High),
                    "OFF" => Ok(VideoQuality::Off),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for WebEgressRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.url.is_empty() {
            len += 1;
        }
        if self.audio_only {
            len += 1;
        }
        if self.video_only {
            len += 1;
        }
        if self.await_start_signal {
            len += 1;
        }
        if !self.file_outputs.is_empty() {
            len += 1;
        }
        if !self.stream_outputs.is_empty() {
            len += 1;
        }
        if !self.segment_outputs.is_empty() {
            len += 1;
        }
        if !self.image_outputs.is_empty() {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        if self.options.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.WebEgressRequest", len)?;
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        if self.audio_only {
            struct_ser.serialize_field("audioOnly", &self.audio_only)?;
        }
        if self.video_only {
            struct_ser.serialize_field("videoOnly", &self.video_only)?;
        }
        if self.await_start_signal {
            struct_ser.serialize_field("awaitStartSignal", &self.await_start_signal)?;
        }
        if !self.file_outputs.is_empty() {
            struct_ser.serialize_field("fileOutputs", &self.file_outputs)?;
        }
        if !self.stream_outputs.is_empty() {
            struct_ser.serialize_field("streamOutputs", &self.stream_outputs)?;
        }
        if !self.segment_outputs.is_empty() {
            struct_ser.serialize_field("segmentOutputs", &self.segment_outputs)?;
        }
        if !self.image_outputs.is_empty() {
            struct_ser.serialize_field("imageOutputs", &self.image_outputs)?;
        }
        if let Some(v) = self.output.as_ref() {
            match v {
                web_egress_request::Output::File(v) => {
                    struct_ser.serialize_field("file", v)?;
                }
                web_egress_request::Output::Stream(v) => {
                    struct_ser.serialize_field("stream", v)?;
                }
                web_egress_request::Output::Segments(v) => {
                    struct_ser.serialize_field("segments", v)?;
                }
            }
        }
        if let Some(v) = self.options.as_ref() {
            match v {
                web_egress_request::Options::Preset(v) => {
                    let v = EncodingOptionsPreset::from_i32(*v)
                        .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("preset", &v)?;
                }
                web_egress_request::Options::Advanced(v) => {
                    struct_ser.serialize_field("advanced", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WebEgressRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "url",
            "audio_only",
            "audioOnly",
            "video_only",
            "videoOnly",
            "await_start_signal",
            "awaitStartSignal",
            "file_outputs",
            "fileOutputs",
            "stream_outputs",
            "streamOutputs",
            "segment_outputs",
            "segmentOutputs",
            "image_outputs",
            "imageOutputs",
            "file",
            "stream",
            "segments",
            "preset",
            "advanced",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Url,
            AudioOnly,
            VideoOnly,
            AwaitStartSignal,
            FileOutputs,
            StreamOutputs,
            SegmentOutputs,
            ImageOutputs,
            File,
            Stream,
            Segments,
            Preset,
            Advanced,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "url" => Ok(GeneratedField::Url),
                            "audioOnly" | "audio_only" => Ok(GeneratedField::AudioOnly),
                            "videoOnly" | "video_only" => Ok(GeneratedField::VideoOnly),
                            "awaitStartSignal" | "await_start_signal" => Ok(GeneratedField::AwaitStartSignal),
                            "fileOutputs" | "file_outputs" => Ok(GeneratedField::FileOutputs),
                            "streamOutputs" | "stream_outputs" => Ok(GeneratedField::StreamOutputs),
                            "segmentOutputs" | "segment_outputs" => Ok(GeneratedField::SegmentOutputs),
                            "imageOutputs" | "image_outputs" => Ok(GeneratedField::ImageOutputs),
                            "file" => Ok(GeneratedField::File),
                            "stream" => Ok(GeneratedField::Stream),
                            "segments" => Ok(GeneratedField::Segments),
                            "preset" => Ok(GeneratedField::Preset),
                            "advanced" => Ok(GeneratedField::Advanced),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WebEgressRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.WebEgressRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<WebEgressRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut url__ = None;
                let mut audio_only__ = None;
                let mut video_only__ = None;
                let mut await_start_signal__ = None;
                let mut file_outputs__ = None;
                let mut stream_outputs__ = None;
                let mut segment_outputs__ = None;
                let mut image_outputs__ = None;
                let mut output__ = None;
                let mut options__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map.next_value()?);
                        }
                        GeneratedField::AudioOnly => {
                            if audio_only__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioOnly"));
                            }
                            audio_only__ = Some(map.next_value()?);
                        }
                        GeneratedField::VideoOnly => {
                            if video_only__.is_some() {
                                return Err(serde::de::Error::duplicate_field("videoOnly"));
                            }
                            video_only__ = Some(map.next_value()?);
                        }
                        GeneratedField::AwaitStartSignal => {
                            if await_start_signal__.is_some() {
                                return Err(serde::de::Error::duplicate_field("awaitStartSignal"));
                            }
                            await_start_signal__ = Some(map.next_value()?);
                        }
                        GeneratedField::FileOutputs => {
                            if file_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fileOutputs"));
                            }
                            file_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::StreamOutputs => {
                            if stream_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("streamOutputs"));
                            }
                            stream_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::SegmentOutputs => {
                            if segment_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segmentOutputs"));
                            }
                            segment_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::ImageOutputs => {
                            if image_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("imageOutputs"));
                            }
                            image_outputs__ = Some(map.next_value()?);
                        }
                        GeneratedField::File => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("file"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(web_egress_request::Output::File)
;
                        }
                        GeneratedField::Stream => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stream"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(web_egress_request::Output::Stream)
;
                        }
                        GeneratedField::Segments => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("segments"));
                            }
                            output__ = map.next_value::<::std::option::Option<_>>()?.map(web_egress_request::Output::Segments)
;
                        }
                        GeneratedField::Preset => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preset"));
                            }
                            options__ = map.next_value::<::std::option::Option<EncodingOptionsPreset>>()?.map(|x| web_egress_request::Options::Preset(x as i32));
                        }
                        GeneratedField::Advanced => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("advanced"));
                            }
                            options__ = map.next_value::<::std::option::Option<_>>()?.map(web_egress_request::Options::Advanced)
;
                        }
                    }
                }
                Ok(WebEgressRequest {
                    url: url__.unwrap_or_default(),
                    audio_only: audio_only__.unwrap_or_default(),
                    video_only: video_only__.unwrap_or_default(),
                    await_start_signal: await_start_signal__.unwrap_or_default(),
                    file_outputs: file_outputs__.unwrap_or_default(),
                    stream_outputs: stream_outputs__.unwrap_or_default(),
                    segment_outputs: segment_outputs__.unwrap_or_default(),
                    image_outputs: image_outputs__.unwrap_or_default(),
                    output: output__,
                    options: options__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.WebEgressRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WebhookEvent {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.event.is_empty() {
            len += 1;
        }
        if self.room.is_some() {
            len += 1;
        }
        if self.participant.is_some() {
            len += 1;
        }
        if self.egress_info.is_some() {
            len += 1;
        }
        if self.ingress_info.is_some() {
            len += 1;
        }
        if self.track.is_some() {
            len += 1;
        }
        if !self.id.is_empty() {
            len += 1;
        }
        if self.created_at != 0 {
            len += 1;
        }
        if self.num_dropped != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.WebhookEvent", len)?;
        if !self.event.is_empty() {
            struct_ser.serialize_field("event", &self.event)?;
        }
        if let Some(v) = self.room.as_ref() {
            struct_ser.serialize_field("room", v)?;
        }
        if let Some(v) = self.participant.as_ref() {
            struct_ser.serialize_field("participant", v)?;
        }
        if let Some(v) = self.egress_info.as_ref() {
            struct_ser.serialize_field("egressInfo", v)?;
        }
        if let Some(v) = self.ingress_info.as_ref() {
            struct_ser.serialize_field("ingressInfo", v)?;
        }
        if let Some(v) = self.track.as_ref() {
            struct_ser.serialize_field("track", v)?;
        }
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if self.created_at != 0 {
            struct_ser.serialize_field("createdAt", ToString::to_string(&self.created_at).as_str())?;
        }
        if self.num_dropped != 0 {
            struct_ser.serialize_field("numDropped", &self.num_dropped)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WebhookEvent {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "event",
            "room",
            "participant",
            "egress_info",
            "egressInfo",
            "ingress_info",
            "ingressInfo",
            "track",
            "id",
            "created_at",
            "createdAt",
            "num_dropped",
            "numDropped",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Event,
            Room,
            Participant,
            EgressInfo,
            IngressInfo,
            Track,
            Id,
            CreatedAt,
            NumDropped,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "event" => Ok(GeneratedField::Event),
                            "room" => Ok(GeneratedField::Room),
                            "participant" => Ok(GeneratedField::Participant),
                            "egressInfo" | "egress_info" => Ok(GeneratedField::EgressInfo),
                            "ingressInfo" | "ingress_info" => Ok(GeneratedField::IngressInfo),
                            "track" => Ok(GeneratedField::Track),
                            "id" => Ok(GeneratedField::Id),
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            "numDropped" | "num_dropped" => Ok(GeneratedField::NumDropped),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WebhookEvent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.WebhookEvent")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<WebhookEvent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut event__ = None;
                let mut room__ = None;
                let mut participant__ = None;
                let mut egress_info__ = None;
                let mut ingress_info__ = None;
                let mut track__ = None;
                let mut id__ = None;
                let mut created_at__ = None;
                let mut num_dropped__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Event => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("event"));
                            }
                            event__ = Some(map.next_value()?);
                        }
                        GeneratedField::Room => {
                            if room__.is_some() {
                                return Err(serde::de::Error::duplicate_field("room"));
                            }
                            room__ = map.next_value()?;
                        }
                        GeneratedField::Participant => {
                            if participant__.is_some() {
                                return Err(serde::de::Error::duplicate_field("participant"));
                            }
                            participant__ = map.next_value()?;
                        }
                        GeneratedField::EgressInfo => {
                            if egress_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("egressInfo"));
                            }
                            egress_info__ = map.next_value()?;
                        }
                        GeneratedField::IngressInfo => {
                            if ingress_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ingressInfo"));
                            }
                            ingress_info__ = map.next_value()?;
                        }
                        GeneratedField::Track => {
                            if track__.is_some() {
                                return Err(serde::de::Error::duplicate_field("track"));
                            }
                            track__ = map.next_value()?;
                        }
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map.next_value()?);
                        }
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NumDropped => {
                            if num_dropped__.is_some() {
                                return Err(serde::de::Error::duplicate_field("numDropped"));
                            }
                            num_dropped__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(WebhookEvent {
                    event: event__.unwrap_or_default(),
                    room: room__,
                    participant: participant__,
                    egress_info: egress_info__,
                    ingress_info: ingress_info__,
                    track: track__,
                    id: id__.unwrap_or_default(),
                    created_at: created_at__.unwrap_or_default(),
                    num_dropped: num_dropped__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.WebhookEvent", FIELDS, GeneratedVisitor)
    }
}
