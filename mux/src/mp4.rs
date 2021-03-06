use std::borrow::Cow;
use std::ffi::CString;

use bytes::{Bytes, BytesMut};
use bytes::buf::BufMutExt;
use mse_fmp4::aac::{AacProfile, SamplingFrequency, ChannelConfiguration};
use mse_fmp4::fmp4::{
    AacSampleEntry, AvcSampleEntry, InitializationSegment, MediaDataBox, MediaSegment,
    Mp4Box, Mpeg4EsDescriptorBox, Sample, SampleEntry, SampleFlags, TrackBox,
    TrackExtendsBox, TrackFragmentBox, MovieFragmentHeaderBox, MovieFragmentBox,
};
use mse_fmp4::io::WriteTo;
use serde::{Deserialize, Serialize};
use mixlab_util::time::{MediaDuration, MediaTime};

#[derive(Debug)]
pub struct Mp4Mux {
    sequence: u32,
    timescale: u32,
    audio_time: MediaTime,
    video_time: MediaTime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdtsFrame(pub Bytes);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AvcFrame {
    pub is_key_frame: bool,
    pub composition_time: MediaDuration,
    pub data: Bytes,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum TrackData {
    Audio(AdtsFrame),
    Video(AvcFrame),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Mp4Params<'a> {
    pub timescale: u32,
    pub width: u32,
    pub height: u32,
    pub dcr: Cow<'a, [u8]>,
}

impl Mp4Mux {
    pub fn new(params: Mp4Params) -> (Self, Bytes) {
        let mux = Mp4Mux {
            sequence: 0,
            timescale: params.timescale,
            audio_time: MediaTime::new(0, 1),
            video_time: MediaTime::new(0, 1),
        };

        let init = make_init_segment(&mux, params);

        (mux, to_bytes(init))
    }

    pub fn write_track(&mut self, duration: MediaDuration, data: &TrackData) -> Bytes {
        let media = make_media_segment(self, duration, data);

        to_bytes(media)
    }
}

fn to_bytes(segment: impl WriteTo) -> Bytes {
    let mut bytes = BytesMut::new();

    // should never fail:
    segment.write_to((&mut bytes).writer()).unwrap();

    bytes.freeze()
}

const AUDIO_TRACK: u32 = 1;
const VIDEO_TRACK: u32 = 2;

fn make_init_segment(
    mux: &Mp4Mux,
    params: Mp4Params,
) -> InitializationSegment {
    use mse_fmp4::fmp4::{
        FileTypeBox, MovieBox, MovieHeaderBox, TrackHeaderBox, MovieExtendsBox,
        MediaBox, MediaHeaderBox, HandlerReferenceBox, MediaInformationBox,
        SoundMediaHeaderBox, DataInformationBox, DataReferenceBox, DataEntryUrlBox,
        SampleTableBox, SampleDescriptionBox, TimeToSampleBox, SampleToChunkBox,
        SampleSizeBox, ChunkOffsetBox, AvcConfigurationBox, VideoMediaHeaderBox,
    };

    InitializationSegment {
        ftyp_box: FileTypeBox,
        moov_box: MovieBox {
            mvhd_box: MovieHeaderBox {
                timescale: mux.timescale,
                // no duration outside of extension fragments:
                duration: 0,
            },
            trak_boxes: vec![
                // audio track:
                TrackBox {
                    tkhd_box: TrackHeaderBox {
                        track_id: AUDIO_TRACK,
                        // ISO/IEC 14496-14:2003(E) 5.3:
                        // If the duration of a track cannot be determined,
                        // then the duration is set to all 1s (32-bit maxint)
                        duration: u32::max_value(),
                        volume: 0x0100, // 16.16 fixed point, 0x0100 = 1.0
                        width: 0,
                        height: 0,
                    },
                    edts_box: None,
                    mdia_box: MediaBox {
                        mdhd_box: MediaHeaderBox {
                            timescale: mux.timescale,
                            duration: 0,
                        },
                        hdlr_box: HandlerReferenceBox {
                            handler_type: *b"soun",
                            name: CString::new("Mixlab Audio").unwrap(),
                        },
                        minf_box: MediaInformationBox {
                            vmhd_box: None,
                            smhd_box: Some(SoundMediaHeaderBox),
                            dinf_box: DataInformationBox {
                                dref_box: DataReferenceBox {
                                    url_box: DataEntryUrlBox,
                                },
                            },
                            stbl_box: SampleTableBox {
                                stsd_box: SampleDescriptionBox {
                                    sample_entries: vec![
                                        SampleEntry::Aac(AacSampleEntry {
                                            esds_box: Mpeg4EsDescriptorBox {
                                                // TODO set these from ADTS header - or are they always constant?
                                                profile: AacProfile::Lc,
                                                frequency: SamplingFrequency::Hz44100,
                                                channel_configuration: ChannelConfiguration::TwoChannels,
                                            },
                                        }),
                                    ],
                                },
                                stts_box: TimeToSampleBox,
                                stsc_box: SampleToChunkBox,
                                stsz_box: SampleSizeBox,
                                stco_box: ChunkOffsetBox,
                            },
                        }
                    },
                },
                // video track:
                TrackBox {
                    tkhd_box: TrackHeaderBox {
                        track_id: VIDEO_TRACK,
                        // ISO/IEC 14496-14:2003(E) 5.3:
                        // If the duration of a track cannot be determined,
                        // then the duration is set to all 1s (32-bit maxint)
                        duration: u32::max_value(),
                        volume: 0x0100, // 16.16 fixed point, 0x0100 = 1.0
                        width: params.width,
                        height: params.height,
                    },
                    edts_box: None,
                    mdia_box: MediaBox {
                        mdhd_box: MediaHeaderBox {
                            timescale: mux.timescale,
                            duration: 0,
                        },
                        hdlr_box: HandlerReferenceBox {
                            handler_type: *b"vide",
                            name: CString::new("Mixlab Video").unwrap(),
                        },
                        minf_box: MediaInformationBox {
                            vmhd_box: Some(VideoMediaHeaderBox),
                            smhd_box: None,
                            dinf_box: DataInformationBox {
                                dref_box: DataReferenceBox {
                                    url_box: DataEntryUrlBox,
                                },
                            },
                            stbl_box: SampleTableBox {
                                stsd_box: SampleDescriptionBox {
                                    sample_entries: vec![
                                        SampleEntry::Avc(AvcSampleEntry {
                                            width: params.width as u16,
                                            height: params.height as u16,
                                            avcc_box: AvcConfigurationBox::Raw(params.dcr.to_vec()),
                                        }),
                                    ],
                                },
                                stts_box: TimeToSampleBox,
                                stsc_box: SampleToChunkBox,
                                stsz_box: SampleSizeBox,
                                stco_box: ChunkOffsetBox,
                            },
                        }
                    },
                },
            ],
            mvex_box: MovieExtendsBox {
                mehd_box: None,
                trex_boxes: vec![
                    TrackExtendsBox {
                        track_id: AUDIO_TRACK,
                        default_sample_description_index: 1,
                        default_sample_duration: 0,
                        default_sample_size: 0,
                        default_sample_flags: 0,
                    },
                    TrackExtendsBox {
                        track_id: VIDEO_TRACK,
                        default_sample_description_index: 1,
                        default_sample_duration: 0,
                        default_sample_size: 0,
                        default_sample_flags: 0,
                    },
                ],
            }
        },
    }
}

fn make_media_segment(
    mux: &mut Mp4Mux,
    duration: MediaDuration,
    track_data: &TrackData,
) -> MediaSegment {
    use mse_fmp4::fmp4::{
        TrackFragmentHeaderBox, TrackRunBox, TrackFragmentBaseMediaDecodeTimeBox,
    };

    let (traf, mdat) = match track_data {
        TrackData::Audio(adts_frame) => {
            let raw_aac = &(adts_frame.0)[7..]; // snip off 7 byte ADTS header

            let time_start_in_mux_base = mux.audio_time.round_to_base(i64::from(mux.timescale));
            let time_end = mux.audio_time + duration;
            let time_end_in_mux_base = time_end.round_to_base(i64::from(mux.timescale));
            let duration_in_mux_base = time_end_in_mux_base - time_start_in_mux_base;
            mux.audio_time = time_end;

            let audio_frag = TrackFragmentBox {
                tfhd_box: TrackFragmentHeaderBox {
                    track_id: AUDIO_TRACK,
                    duration_is_empty: false,
                    default_base_is_moof: true,
                    base_data_offset: None,
                    sample_description_index: None,
                    default_sample_duration: None,
                    default_sample_size: None,
                    default_sample_flags: None,
                },
                tfdt_box: Some(TrackFragmentBaseMediaDecodeTimeBox {
                    base_media_decode_time: time_start_in_mux_base as u32,
                }),
                trun_box: TrackRunBox {
                    data_offset: Some(0), // dummy for length calculation
                    first_sample_flags: None,
                    samples: vec![Sample {
                        duration: Some(duration_in_mux_base as u32),
                        size: Some(raw_aac.len() as u32),
                        composition_time_offset: None,
                        flags: None,
                    }],
                }
            };

            (audio_frag, MediaDataBox {
                // TODO - remove to_vec and borrow here:
                data: raw_aac.to_vec(),
            })
        }
        TrackData::Video(avc_frame) => {
            let sample_flags = SampleFlags {
                is_leading: 0,
                // ISO/IEC 14496-12 8.40.2.3, other samples depend on this:
                sample_depends_on: 1,
                // ISO/IEC 14496-12 8.31.1, false signals a key frame:
                sample_is_non_sync_sample: !avc_frame.is_key_frame,
                // should this be 1?
                sample_is_depdended_on: 0,
                sample_has_redundancy: 0,
                sample_padding_value: 0,
                sample_degradation_priority: 0,
            };

            let time_start_in_mux_base = mux.video_time.round_to_base(i64::from(mux.timescale));
            let time_end = mux.video_time + duration;
            let time_end_in_mux_base = time_end.round_to_base(i64::from(mux.timescale));
            let duration_in_mux_base = time_end_in_mux_base - time_start_in_mux_base;
            mux.video_time = time_end;

            let video_frag = TrackFragmentBox {
                tfhd_box: TrackFragmentHeaderBox {
                    track_id: VIDEO_TRACK,
                    duration_is_empty: false,
                    default_base_is_moof: true,
                    base_data_offset: None,
                    sample_description_index: None,
                    default_sample_duration: None,
                    default_sample_size: None,
                    default_sample_flags: None,
                },
                tfdt_box: Some(TrackFragmentBaseMediaDecodeTimeBox {
                    base_media_decode_time: time_start_in_mux_base as u32, // TODO is this affected by composition time?
                }),
                trun_box: TrackRunBox {
                    data_offset: Some(0), // dummy for length calculation
                    first_sample_flags: None,
                    samples: vec![Sample {
                        duration: Some(duration_in_mux_base as u32),
                        size: Some(avc_frame.data.len() as u32),
                        composition_time_offset: Some(avc_frame.composition_time.round_to_base(i64::from(mux.timescale)) as i32),
                        flags: Some(sample_flags),
                    }],
                }
            };

            (video_frag, MediaDataBox {
                data: avc_frame.data.to_vec(),
            })
        }
    };

    let mut segment = MediaSegment {
        moof_box: MovieFragmentBox {
            mfhd_box: MovieFragmentHeaderBox {
                sequence_number: {
                    mux.sequence += 1;
                    mux.sequence
                },
            },
            traf_boxes: vec![traf],
        },
        mdat_boxes: vec![mdat],
    };

    let moof_size = segment.moof_box.box_size().unwrap();

    segment.moof_box.traf_boxes[0].trun_box.data_offset =
        // +8 accounts for header in new mdat box:
        Some(moof_size as i32 + 8);

    segment
}
