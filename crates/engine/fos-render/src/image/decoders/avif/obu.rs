//! AV1 OBU (Open Bitstream Unit) Parser
//!
//! Parses AV1 bitstream structure including sequence and frame headers.

use super::bitreader::BitReader;
use super::AvifError;

/// OBU types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObuType {
    SequenceHeader,
    TemporalDelimiter,
    FrameHeader,
    TileGroup,
    Metadata,
    Frame,
    RedundantFrameHeader,
    TileList,
    Padding,
    Unknown(u8),
}

impl ObuType {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::SequenceHeader,
            2 => Self::TemporalDelimiter,
            3 => Self::FrameHeader,
            4 => Self::TileGroup,
            5 => Self::Metadata,
            6 => Self::Frame,
            7 => Self::RedundantFrameHeader,
            8 => Self::TileList,
            15 => Self::Padding,
            _ => Self::Unknown(v),
        }
    }
}

/// Sequence header parameters
#[derive(Debug, Clone)]
pub struct SequenceHeader {
    pub profile: u8,
    pub still_picture: bool,
    pub reduced_still_picture_header: bool,
    pub max_frame_width: u32,
    pub max_frame_height: u32,
    pub bit_depth: u8,
    pub monochrome: bool,
    pub subsampling_x: u8,
    pub subsampling_y: u8,
    pub use_128x128_superblock: bool,
    pub enable_filter_intra: bool,
    pub enable_intra_edge_filter: bool,
    pub enable_masked_compound: bool,
    pub enable_warped_motion: bool,
    pub enable_dual_filter: bool,
    pub enable_jnt_comp: bool,
    pub enable_ref_frame_mvs: bool,
    pub enable_superres: bool,
    pub enable_cdef: bool,
    pub enable_restoration: bool,
    pub color_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub color_range: bool,
    pub chroma_sample_position: u8,
}

impl Default for SequenceHeader {
    fn default() -> Self {
        Self {
            profile: 0,
            still_picture: true,
            reduced_still_picture_header: true,
            max_frame_width: 0,
            max_frame_height: 0,
            bit_depth: 8,
            monochrome: false,
            subsampling_x: 1,
            subsampling_y: 1,
            use_128x128_superblock: false,
            enable_filter_intra: true,
            enable_intra_edge_filter: true,
            enable_masked_compound: false,
            enable_warped_motion: false,
            enable_dual_filter: false,
            enable_jnt_comp: false,
            enable_ref_frame_mvs: false,
            enable_superres: false,
            enable_cdef: true,
            enable_restoration: true,
            color_primaries: 1,
            transfer_characteristics: 13,
            matrix_coefficients: 6,
            color_range: false,
            chroma_sample_position: 0,
        }
    }
}

/// Frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Key,
    Inter,
    IntraOnly,
    Switch,
}

/// Quantization parameters
#[derive(Debug, Clone, Default)]
pub struct QuantizationParams {
    pub base_q_idx: u8,
    pub delta_q_y_dc: i8,
    pub delta_q_u_dc: i8,
    pub delta_q_u_ac: i8,
    pub delta_q_v_dc: i8,
    pub delta_q_v_ac: i8,
    pub using_qmatrix: bool,
    pub qm_y: u8,
    pub qm_u: u8,
    pub qm_v: u8,
}

/// Loop filter parameters
#[derive(Debug, Clone, Default)]
pub struct LoopFilterParams {
    pub enabled: bool,
    pub level: [u8; 4],
    pub sharpness: u8,
    pub delta_enabled: bool,
    pub delta_update: bool,
    pub ref_deltas: [i8; 8],
    pub mode_deltas: [i8; 2],
}

/// CDEF parameters
#[derive(Debug, Clone, Default)]
pub struct CdefParams {
    pub enabled: bool,
    pub damping: u8,
    pub bits: u8,
    pub y_pri_strength: [u8; 8],
    pub y_sec_strength: [u8; 8],
    pub uv_pri_strength: [u8; 8],
    pub uv_sec_strength: [u8; 8],
}

/// Loop restoration parameters
#[derive(Debug, Clone, Default)]
pub struct RestorationParams {
    pub enabled: bool,
    pub type_y: RestorationType,
    pub type_u: RestorationType,
    pub type_v: RestorationType,
    pub unit_size: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RestorationType {
    #[default]
    None,
    Wiener,
    SelfGuided,
    Switchable,
}

/// Tile info
#[derive(Debug, Clone, Default)]
pub struct TileInfo {
    pub tile_cols: u32,
    pub tile_rows: u32,
    pub tile_width_sb: u32,
    pub tile_height_sb: u32,
    pub context_update_tile_id: u32,
}

/// Frame header
#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub frame_type: FrameType,
    pub show_frame: bool,
    pub showable_frame: bool,
    pub error_resilient_mode: bool,
    pub disable_cdf_update: bool,
    pub frame_width: u32,
    pub frame_height: u32,
    pub render_width: u32,
    pub render_height: u32,
    pub quantization: QuantizationParams,
    pub loop_filter: LoopFilterParams,
    pub cdef: CdefParams,
    pub restoration: RestorationParams,
    pub tile_info: TileInfo,
    pub tx_mode: TxMode,
}

impl Default for FrameHeader {
    fn default() -> Self {
        Self {
            frame_type: FrameType::Key,
            show_frame: true,
            showable_frame: true,
            error_resilient_mode: false,
            disable_cdf_update: false,
            frame_width: 0,
            frame_height: 0,
            render_width: 0,
            render_height: 0,
            quantization: QuantizationParams::default(),
            loop_filter: LoopFilterParams::default(),
            cdef: CdefParams::default(),
            restoration: RestorationParams::default(),
            tile_info: TileInfo::default(),
            tx_mode: TxMode::Select,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TxMode {
    Only4x4,
    Largest,
    #[default]
    Select,
}

/// Tile data
#[derive(Debug, Clone)]
pub struct TileData {
    pub row: usize,
    pub col: usize,
    pub data: Vec<u8>,
}

/// Parse all OBUs from AV1 data
pub fn parse_obus(data: &[u8]) -> Result<(SequenceHeader, FrameHeader, Vec<TileData>), AvifError> {
    let mut parser = ObuParser::new(data);
    parser.parse()
}

struct ObuParser<'a> {
    reader: BitReader<'a>,
    data: &'a [u8],
    seq_header: Option<SequenceHeader>,
    frame_header: Option<FrameHeader>,
    tile_data: Vec<TileData>,
}

impl<'a> ObuParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            reader: BitReader::new(data),
            data,
            seq_header: None,
            frame_header: None,
            tile_data: Vec::new(),
        }
    }
    
    fn parse(&mut self) -> Result<(SequenceHeader, FrameHeader, Vec<TileData>), AvifError> {
        while !self.reader.is_empty() {
            self.parse_obu()?;
        }
        
        let seq_header = self.seq_header.take()
            .ok_or_else(|| AvifError::ObuParseError("Missing sequence header".into()))?;
        let frame_header = self.frame_header.take()
            .ok_or_else(|| AvifError::ObuParseError("Missing frame header".into()))?;
        
        Ok((seq_header, frame_header, std::mem::take(&mut self.tile_data)))
    }
    
    fn parse_obu(&mut self) -> Result<(), AvifError> {
        let start_pos = self.reader.position();
        
        // OBU header
        let _forbidden = self.reader.read_bit()?;
        let obu_type = ObuType::from_u8(self.reader.read_bits(4)? as u8);
        let extension_flag = self.reader.read_bit()?;
        let has_size_field = self.reader.read_bit()?;
        let _reserved = self.reader.read_bit()?;
        
        if extension_flag {
            // Skip extension header
            self.reader.read_bits(8)?;
        }
        
        let obu_size = if has_size_field {
            self.reader.read_leb128()? as usize
        } else {
            self.reader.remaining()
        };
        
        let header_size = self.reader.position() - start_pos;
        let obu_end = self.reader.position() + obu_size;
        
        match obu_type {
            ObuType::SequenceHeader => {
                self.parse_sequence_header()?;
            }
            ObuType::FrameHeader => {
                self.parse_frame_header()?;
            }
            ObuType::Frame => {
                // Frame OBU contains frame header + tile group
                self.parse_frame_header()?;
                self.parse_tile_group(obu_end)?;
            }
            ObuType::TileGroup => {
                self.parse_tile_group(obu_end)?;
            }
            ObuType::Metadata => {
                // Skip metadata for now
            }
            ObuType::TemporalDelimiter | ObuType::Padding => {
                // Skip
            }
            _ => {}
        }
        
        // Ensure we're at the end of the OBU
        let current_pos = self.reader.position();
        if current_pos < obu_end {
            self.reader.byte_align();
            let skip = obu_end - self.reader.position();
            if skip > 0 {
                self.reader.skip_bits((skip * 8) as u32)?;
            }
        }
        
        Ok(())
    }
    
    fn parse_sequence_header(&mut self) -> Result<(), AvifError> {
        let mut seq = SequenceHeader::default();
        
        seq.profile = self.reader.read_bits(3)? as u8;
        seq.still_picture = self.reader.read_bit()?;
        seq.reduced_still_picture_header = self.reader.read_bit()?;
        
        if seq.reduced_still_picture_header {
            // Simplified header for still pictures
            let _timing_info_present = false;
            let _decoder_model_info_present = false;
            let _initial_display_delay_present = false;
            let operating_points_cnt = 1;
            let _operating_point_idc = 0u16;
            let seq_level_idx = self.reader.read_bits(5)? as u8;
            let _seq_tier = 0u8;
            let _decoder_model_present = false;
            let _initial_display_delay_minus1 = 0u8;
        } else {
            // Full header parsing
            let timing_info_present = self.reader.read_bit()?;
            
            if timing_info_present {
                // Parse timing info
                let _num_units_in_display_tick = self.reader.read_bits(32)?;
                let _time_scale = self.reader.read_bits(32)?;
                let equal_picture_interval = self.reader.read_bit()?;
                if equal_picture_interval {
                    let _num_ticks = self.reader.read_uvlc()?;
                }
                
                let decoder_model_info_present = self.reader.read_bit()?;
                if decoder_model_info_present {
                    let _buffer_delay_length = self.reader.read_bits(5)?;
                    let _num_units_in_decoding_tick = self.reader.read_bits(32)?;
                    let _buffer_removal_time_length = self.reader.read_bits(5)?;
                    let _frame_presentation_time_length = self.reader.read_bits(5)?;
                }
            }
            
            let initial_display_delay_present = self.reader.read_bit()?;
            let operating_points_cnt = self.reader.read_bits(5)? as usize + 1;
            
            for _ in 0..operating_points_cnt {
                let _operating_point_idc = self.reader.read_bits(12)?;
                let _seq_level_idx = self.reader.read_bits(5)?;
                if self.reader.read_bits(5)? > 7 {
                    let _seq_tier = self.reader.read_bit()?;
                }
                // Skip decoder model and display delay
                if timing_info_present {
                    let _decoder_model_present = self.reader.read_bit()?;
                }
                if initial_display_delay_present {
                    let has_delay = self.reader.read_bit()?;
                    if has_delay {
                        let _delay = self.reader.read_bits(4)?;
                    }
                }
            }
        }
        
        // Frame size
        let frame_width_bits = self.reader.read_bits(4)? as u8 + 1;
        let frame_height_bits = self.reader.read_bits(4)? as u8 + 1;
        seq.max_frame_width = self.reader.read_bits(frame_width_bits)? + 1;
        seq.max_frame_height = self.reader.read_bits(frame_height_bits)? + 1;
        
        // Frame ID
        let frame_id_numbers_present = if !seq.reduced_still_picture_header {
            self.reader.read_bit()?
        } else {
            false
        };
        
        if frame_id_numbers_present {
            let _delta_frame_id_length = self.reader.read_bits(4)? + 2;
            let _additional_frame_id_length = self.reader.read_bits(3)? + 1;
        }
        
        // Feature flags
        seq.use_128x128_superblock = self.reader.read_bit()?;
        seq.enable_filter_intra = self.reader.read_bit()?;
        seq.enable_intra_edge_filter = self.reader.read_bit()?;
        
        if !seq.reduced_still_picture_header {
            seq.enable_masked_compound = self.reader.read_bit()?;
            seq.enable_warped_motion = self.reader.read_bit()?;
            seq.enable_dual_filter = self.reader.read_bit()?;
            
            let enable_order_hint = self.reader.read_bit()?;
            if enable_order_hint {
                seq.enable_jnt_comp = self.reader.read_bit()?;
                seq.enable_ref_frame_mvs = self.reader.read_bit()?;
            }
            
            let seq_choose_screen_content_tools = self.reader.read_bit()?;
            let _seq_force_screen_content_tools = if seq_choose_screen_content_tools {
                2
            } else {
                self.reader.read_bits(1)?
            };
            
            let seq_choose_integer_mv = self.reader.read_bit()?;
            let _seq_force_integer_mv = if seq_choose_integer_mv {
                2
            } else {
                self.reader.read_bits(1)?
            };
            
            if enable_order_hint {
                let _order_hint_bits = self.reader.read_bits(3)? + 1;
            }
        }
        
        seq.enable_superres = self.reader.read_bit()?;
        seq.enable_cdef = self.reader.read_bit()?;
        seq.enable_restoration = self.reader.read_bit()?;
        
        // Color config
        self.parse_color_config(&mut seq)?;
        
        let _film_grain_params_present = self.reader.read_bit()?;
        
        self.seq_header = Some(seq);
        Ok(())
    }
    
    fn parse_color_config(&mut self, seq: &mut SequenceHeader) -> Result<(), AvifError> {
        let high_bitdepth = self.reader.read_bit()?;
        
        if seq.profile == 2 && high_bitdepth {
            let twelve_bit = self.reader.read_bit()?;
            seq.bit_depth = if twelve_bit { 12 } else { 10 };
        } else {
            seq.bit_depth = if high_bitdepth { 10 } else { 8 };
        }
        
        if seq.profile == 1 {
            seq.monochrome = false;
        } else {
            seq.monochrome = self.reader.read_bit()?;
        }
        
        let color_description_present = self.reader.read_bit()?;
        
        if color_description_present {
            seq.color_primaries = self.reader.read_bits(8)? as u8;
            seq.transfer_characteristics = self.reader.read_bits(8)? as u8;
            seq.matrix_coefficients = self.reader.read_bits(8)? as u8;
        } else {
            seq.color_primaries = 2; // Unspecified
            seq.transfer_characteristics = 2;
            seq.matrix_coefficients = 2;
        }
        
        if seq.monochrome {
            seq.color_range = self.reader.read_bit()?;
            seq.subsampling_x = 1;
            seq.subsampling_y = 1;
            seq.chroma_sample_position = 0;
        } else if seq.color_primaries == 1 && 
                   seq.transfer_characteristics == 13 && 
                   seq.matrix_coefficients == 0 {
            // sRGB
            seq.color_range = true;
            seq.subsampling_x = 0;
            seq.subsampling_y = 0;
        } else {
            seq.color_range = self.reader.read_bit()?;
            
            if seq.profile == 0 {
                seq.subsampling_x = 1;
                seq.subsampling_y = 1;
            } else if seq.profile == 1 {
                seq.subsampling_x = 0;
                seq.subsampling_y = 0;
            } else {
                if seq.bit_depth == 12 {
                    seq.subsampling_x = self.reader.read_bit()? as u8;
                    if seq.subsampling_x == 1 {
                        seq.subsampling_y = self.reader.read_bit()? as u8;
                    } else {
                        seq.subsampling_y = 0;
                    }
                } else {
                    seq.subsampling_x = 1;
                    seq.subsampling_y = 0;
                }
            }
            
            if seq.subsampling_x == 1 && seq.subsampling_y == 1 {
                seq.chroma_sample_position = self.reader.read_bits(2)? as u8;
            }
        }
        
        let _separate_uv_delta_q = if !seq.monochrome {
            self.reader.read_bit()?
        } else {
            false
        };
        
        Ok(())
    }
    
    fn parse_frame_header(&mut self) -> Result<(), AvifError> {
        let seq = self.seq_header.as_ref()
            .ok_or_else(|| AvifError::ObuParseError("Frame header before sequence header".into()))?
            .clone();
        
        let mut frame = FrameHeader::default();
        
        if seq.reduced_still_picture_header {
            frame.frame_type = FrameType::Key;
            frame.show_frame = true;
            frame.showable_frame = false;
        } else {
            let show_existing_frame = self.reader.read_bit()?;
            
            if show_existing_frame {
                // Reference an existing frame
                let _frame_to_show = self.reader.read_bits(3)?;
                self.frame_header = Some(frame);
                return Ok(());
            }
            
            let frame_type_bits = self.reader.read_bits(2)?;
            frame.frame_type = match frame_type_bits {
                0 => FrameType::Key,
                1 => FrameType::Inter,
                2 => FrameType::IntraOnly,
                _ => FrameType::Switch,
            };
            
            frame.show_frame = self.reader.read_bit()?;
            
            if frame.show_frame {
                // Parse more header info
            }
            
            frame.error_resilient_mode = if frame.frame_type == FrameType::Switch {
                true
            } else if frame.frame_type == FrameType::Key && frame.show_frame {
                true
            } else {
                self.reader.read_bit()?
            };
        }
        
        frame.disable_cdf_update = self.reader.read_bit()?;
        
        // Frame size
        frame.frame_width = seq.max_frame_width;
        frame.frame_height = seq.max_frame_height;
        frame.render_width = frame.frame_width;
        frame.render_height = frame.frame_height;
        
        // Parse remaining frame header elements
        self.parse_quantization_params(&mut frame, &seq)?;
        self.parse_loop_filter_params(&mut frame, &seq)?;
        self.parse_cdef_params(&mut frame, &seq)?;
        self.parse_restoration_params(&mut frame, &seq)?;
        
        // TX mode
        if !self.reader.is_empty() {
            let tx_mode_select = self.reader.read_bit()?;
            frame.tx_mode = if tx_mode_select {
                TxMode::Select
            } else {
                TxMode::Largest
            };
        }
        
        // Tile info
        self.parse_tile_info(&mut frame, &seq)?;
        
        self.frame_header = Some(frame);
        Ok(())
    }
    
    fn parse_quantization_params(&mut self, frame: &mut FrameHeader, seq: &SequenceHeader) -> Result<(), AvifError> {
        frame.quantization.base_q_idx = self.reader.read_bits(8)? as u8;
        
        frame.quantization.delta_q_y_dc = self.read_delta_q()?;
        
        if !seq.monochrome {
            let diff_uv_delta = self.reader.read_bit()?;
            
            frame.quantization.delta_q_u_dc = self.read_delta_q()?;
            frame.quantization.delta_q_u_ac = self.read_delta_q()?;
            
            if diff_uv_delta {
                frame.quantization.delta_q_v_dc = self.read_delta_q()?;
                frame.quantization.delta_q_v_ac = self.read_delta_q()?;
            } else {
                frame.quantization.delta_q_v_dc = frame.quantization.delta_q_u_dc;
                frame.quantization.delta_q_v_ac = frame.quantization.delta_q_u_ac;
            }
        }
        
        frame.quantization.using_qmatrix = self.reader.read_bit()?;
        
        if frame.quantization.using_qmatrix {
            frame.quantization.qm_y = self.reader.read_bits(4)? as u8;
            frame.quantization.qm_u = self.reader.read_bits(4)? as u8;
            if !seq.monochrome {
                frame.quantization.qm_v = self.reader.read_bits(4)? as u8;
            } else {
                frame.quantization.qm_v = frame.quantization.qm_u;
            }
        }
        
        Ok(())
    }
    
    fn read_delta_q(&mut self) -> Result<i8, AvifError> {
        let has_delta = self.reader.read_bit()?;
        if has_delta {
            Ok(self.reader.read_su(7)? as i8)
        } else {
            Ok(0)
        }
    }
    
    fn parse_loop_filter_params(&mut self, frame: &mut FrameHeader, _seq: &SequenceHeader) -> Result<(), AvifError> {
        frame.loop_filter.level[0] = self.reader.read_bits(6)? as u8;
        frame.loop_filter.level[1] = self.reader.read_bits(6)? as u8;
        
        frame.loop_filter.enabled = frame.loop_filter.level[0] > 0 || frame.loop_filter.level[1] > 0;
        
        if frame.loop_filter.enabled {
            frame.loop_filter.level[2] = self.reader.read_bits(6)? as u8;
            frame.loop_filter.level[3] = self.reader.read_bits(6)? as u8;
        }
        
        frame.loop_filter.sharpness = self.reader.read_bits(3)? as u8;
        
        frame.loop_filter.delta_enabled = self.reader.read_bit()?;
        
        if frame.loop_filter.delta_enabled {
            frame.loop_filter.delta_update = self.reader.read_bit()?;
            
            if frame.loop_filter.delta_update {
                for i in 0..8 {
                    let has_delta = self.reader.read_bit()?;
                    if has_delta {
                        frame.loop_filter.ref_deltas[i] = self.reader.read_su(7)? as i8;
                    }
                }
                
                for i in 0..2 {
                    let has_delta = self.reader.read_bit()?;
                    if has_delta {
                        frame.loop_filter.mode_deltas[i] = self.reader.read_su(7)? as i8;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn parse_cdef_params(&mut self, frame: &mut FrameHeader, seq: &SequenceHeader) -> Result<(), AvifError> {
        if !seq.enable_cdef {
            frame.cdef.enabled = false;
            return Ok(());
        }
        
        frame.cdef.enabled = true;
        frame.cdef.damping = self.reader.read_bits(2)? as u8 + 3;
        frame.cdef.bits = self.reader.read_bits(2)? as u8;
        
        let num_strengths = 1 << frame.cdef.bits;
        
        for i in 0..num_strengths {
            frame.cdef.y_pri_strength[i] = self.reader.read_bits(4)? as u8;
            frame.cdef.y_sec_strength[i] = self.reader.read_bits(2)? as u8;
            if frame.cdef.y_sec_strength[i] == 3 {
                frame.cdef.y_sec_strength[i] = 4;
            }
            
            if !seq.monochrome {
                frame.cdef.uv_pri_strength[i] = self.reader.read_bits(4)? as u8;
                frame.cdef.uv_sec_strength[i] = self.reader.read_bits(2)? as u8;
                if frame.cdef.uv_sec_strength[i] == 3 {
                    frame.cdef.uv_sec_strength[i] = 4;
                }
            }
        }
        
        Ok(())
    }
    
    fn parse_restoration_params(&mut self, frame: &mut FrameHeader, seq: &SequenceHeader) -> Result<(), AvifError> {
        if !seq.enable_restoration {
            frame.restoration.enabled = false;
            return Ok(());
        }
        
        let mut any_enabled = false;
        
        for i in 0..3 {
            if i == 0 || !seq.monochrome {
                let lr_type = self.reader.read_bits(2)?;
                let rtype = match lr_type {
                    0 => RestorationType::None,
                    1 => RestorationType::Wiener,
                    2 => RestorationType::SelfGuided,
                    _ => RestorationType::Switchable,
                };
                
                match i {
                    0 => frame.restoration.type_y = rtype,
                    1 => frame.restoration.type_u = rtype,
                    _ => frame.restoration.type_v = rtype,
                }
                
                if rtype != RestorationType::None {
                    any_enabled = true;
                }
            }
        }
        
        frame.restoration.enabled = any_enabled;
        
        if any_enabled {
            let sb_size = if seq.use_128x128_superblock { 128 } else { 64 };
            
            if seq.use_128x128_superblock {
                let lr_unit_shift = self.reader.read_bits(1)?;
                frame.restoration.unit_size = sb_size >> (1 - lr_unit_shift);
            } else {
                let lr_unit_shift = self.reader.read_bits(1)?;
                if lr_unit_shift != 0 {
                    let lr_unit_extra_shift = self.reader.read_bits(1)?;
                    frame.restoration.unit_size = sb_size >> (2 - lr_unit_extra_shift);
                } else {
                    frame.restoration.unit_size = sb_size >> 2;
                }
            }
        }
        
        Ok(())
    }
    
    fn parse_tile_info(&mut self, frame: &mut FrameHeader, seq: &SequenceHeader) -> Result<(), AvifError> {
        let sb_size = if seq.use_128x128_superblock { 128 } else { 64 };
        let sb_cols = (frame.frame_width + sb_size - 1) / sb_size;
        let sb_rows = (frame.frame_height + sb_size - 1) / sb_size;
        
        let uniform_tile_spacing = self.reader.read_bit()?;
        
        if uniform_tile_spacing {
            // Calculate tile dimensions
            let mut tile_width_sb = sb_cols;
            let mut tile_height_sb = sb_rows;
            
            loop {
                if tile_width_sb > 1 {
                    let _increment = self.reader.read_bit()?;
                    // In a real implementation, we'd recursively halve
                    break;
                } else {
                    break;
                }
            }
            
            loop {
                if tile_height_sb > 1 {
                    let _increment = self.reader.read_bit()?;
                    break;
                } else {
                    break;
                }
            }
            
            frame.tile_info.tile_width_sb = tile_width_sb.max(1);
            frame.tile_info.tile_height_sb = tile_height_sb.max(1);
            frame.tile_info.tile_cols = (sb_cols + frame.tile_info.tile_width_sb - 1) / frame.tile_info.tile_width_sb;
            frame.tile_info.tile_rows = (sb_rows + frame.tile_info.tile_height_sb - 1) / frame.tile_info.tile_height_sb;
        } else {
            // Non-uniform tiles - parse explicit sizes
            frame.tile_info.tile_cols = 1;
            frame.tile_info.tile_rows = 1;
            frame.tile_info.tile_width_sb = sb_cols;
            frame.tile_info.tile_height_sb = sb_rows;
        }
        
        if frame.tile_info.tile_cols * frame.tile_info.tile_rows > 1 {
            let tile_bits = (frame.tile_info.tile_cols * frame.tile_info.tile_rows).ilog2() as u8;
            frame.tile_info.context_update_tile_id = self.reader.read_bits(tile_bits)? .min(frame.tile_info.tile_cols * frame.tile_info.tile_rows - 1);
        }
        
        Ok(())
    }
    
    fn parse_tile_group(&mut self, obu_end: usize) -> Result<(), AvifError> {
        let frame = self.frame_header.as_ref()
            .ok_or_else(|| AvifError::ObuParseError("Tile group before frame header".into()))?;
        
        let num_tiles = frame.tile_info.tile_cols * frame.tile_info.tile_rows;
        
        let (tile_start, tile_end) = if num_tiles > 1 {
            let tile_bits = num_tiles.ilog2() as u8;
            let start = self.reader.read_bits(tile_bits)?;
            let end = self.reader.read_bits(tile_bits)?;
            (start as usize, end as usize)
        } else {
            (0, 0)
        };
        
        self.reader.byte_align();
        
        // Read tile data
        for tile_idx in tile_start..=tile_end {
            let row = tile_idx as u32 / frame.tile_info.tile_cols;
            let col = tile_idx as u32 % frame.tile_info.tile_cols;
            
            let tile_size = if tile_idx == tile_end {
                // Last tile goes to end of OBU
                obu_end - self.reader.position()
            } else {
                // Read tile size (LE encoded)
                let size_bytes = 4; // Simplified, actual uses tile_size_bytes_minus_1
                let mut size = 0usize;
                for i in 0..size_bytes {
                    size |= (self.reader.read_bits(8)? as usize) << (i * 8);
                }
                size + 1
            };
            
            let tile_data = self.reader.read_bytes(tile_size.min(self.reader.remaining()))?;
            
            self.tile_data.push(TileData {
                row: row as usize,
                col: col as usize,
                data: tile_data,
            });
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_obu_type() {
        assert_eq!(ObuType::from_u8(1), ObuType::SequenceHeader);
        assert_eq!(ObuType::from_u8(6), ObuType::Frame);
        assert!(matches!(ObuType::from_u8(99), ObuType::Unknown(99)));
    }
    
    #[test]
    fn test_sequence_header_default() {
        let seq = SequenceHeader::default();
        assert_eq!(seq.bit_depth, 8);
        assert!(seq.still_picture);
        assert!(seq.enable_cdef);
    }
    
    #[test]
    fn test_frame_type() {
        let frame = FrameHeader::default();
        assert_eq!(frame.frame_type, FrameType::Key);
    }
}
