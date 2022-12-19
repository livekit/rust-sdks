#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    url: String,
    token: String,
    options: SignalOptions,
    join_response: JoinResponse,
}

/// This struct holds a WebRTC session
/// The session changes at every reconnection
#[derive(Debug)]
pub struct RTCSession {
    info: SessionInfo,
    publisher_pc: AsyncMutex<PCTransport>,
    subscriber_pc: AsyncMutex<PCTransport>,

    // Publisher data channels
    // Used to send data to other participants ( The SFU forwards the messages )
    lossy_dc: DataChannel,
    reliable_dc: DataChannel,

    // Subscriber data channels
    // These fields are never used, we just keep a strong reference to them,
    // so we can receive data from other participants
    sub_reliable_dc: Mutex<Option<DataChannel>>,
    sub_lossy_dc: Mutex<Option<DataChannel>>,
}

impl RTCSession {
    pub fn new(
        lk_runtime: Arc<LKRuntime>,
        session_info: SessionInfo,
    ) -> EngineResult<(Self, RTCEvents)> {
        let (rtc_emitter, events) = mpsc::unbounded_channel();
        let rtc_config = RTCConfiguration::from(session_info.join_response);

        let mut publisher_pc = PCTransport::new(
            lk_runtime
                .pc_factory
                .create_peer_connection(rtc_config.clone())?,
            SignalTarget::Publisher,
        );

        let mut subscriber_pc = PCTransport::new(
            lk_runtime
                .pc_factory
                .create_peer_connection(rtc_config.clone())?,
            SignalTarget::Subscriber,
        );

        let mut lossy_dc = publisher_pc.peer_connection().create_data_channel(
            LOSSY_DC_LABEL,
            DataChannelInit {
                ordered: true,
                max_retransmits: Some(0),
                ..DataChannelInit::default()
            },
        )?;

        let mut reliable_dc = publisher_pc.peer_connection().create_data_channel(
            RELIABLE_DC_LABEL,
            DataChannelInit {
                ordered: true,
                ..DataChannelInit::default()
            },
        )?;

        rtc_events::forward_pc_events(&mut publisher_pc, rtc_emitter.clone());
        rtc_events::forward_pc_events(&mut subscriber_pc, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut lossy_dc, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut reliable_dc, rtc_emitter.clone());

        Ok((
            Self {
                info: session_info,
                publisher_pc: AsyncMutex::new(publisher_pc),
                subscriber_pc: AsyncMutex::new(subscriber_pc),
                sub_lossy_dc: Default::default(),
                sub_reliable_dc: Default::default(),
                lossy_dc,
                reliable_dc,
            },
            events,
        ))
    }

    
}
