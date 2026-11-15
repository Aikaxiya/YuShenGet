//! 欧皇度评级。
//!
//! 说明：官方没有这套评分，所有第三方工具的「欧皇度」都是各自定义的。
//! 这里给出一个完全公开、可复核的公式，并把各分项一并返回，
//! UI 上会展示「评分构成」，避免出现无法解释的黑箱数字。

use serde::Serialize;

/// 角色池 UP 的理论期望抽数：93.75 = 62.5（出金期望）÷ (1/1.5)，
/// 其中 1/1.5 来自 50% 小保底歪率。用作「欧/非」的基准线。
const EXPECTED_UP_PITY: f64 = 93.75;

#[derive(Debug, Clone, Serialize)]
pub struct Rating {
    /// 0..100，越高越欧。
    pub score: f64,
    pub label: &'static str,
    pub tier: u8,
    /// 出金节奏分项（0..100）。
    pub pity_component: f64,
    /// 歪率分项（0..100）。
    pub luck_component: f64,
    /// 连歪惩罚扣分。
    pub penalty: f64,
    /// 样本是否足够（少于 100 抽不建议看评级）。
    pub sample_sufficient: bool,
}

pub struct RatingInput {
    pub pulls: u32,
    pub avg_up_pity: Option<f64>,
    pub not_off_rate: Option<f64>,
    pub max_off_streak: u32,
}

pub fn evaluate(input: RatingInput) -> Rating {
    let sample_sufficient = input.pulls >= 100;

    // 出金节奏：实际 UP 平均抽数优于期望则加分，劣于则减分，100 为上限。
    let pity_component = match input.avg_up_pity {
        Some(avg) if avg > 0.0 => {
            let ratio = EXPECTED_UP_PITY / avg;
            (ratio * 50.0).clamp(0.0, 100.0)
        }
        _ => 50.0,
    };

    // 歪率分项：50% 为基准线（50 分），全歪 0 分，全不歪 100 分。
    let luck_component = match input.not_off_rate {
        Some(rate) => (rate * 100.0).clamp(0.0, 100.0),
        None => 50.0,
    };

    // 连歪惩罚：超过 2 连歪才开始扣分，每多一连连扣 4 分，最多扣 20 分。
    let penalty = if input.max_off_streak > 2 {
        (((input.max_off_streak - 2) as f64) * 4.0).min(20.0)
    } else {
        0.0
    };

    let score = (0.5 * pity_component + 0.5 * luck_component - penalty).clamp(0.0, 100.0);

    let (label, tier) = match score {
        s if s >= 82.0 => ("大欧皇", 0),
        s if s >= 68.0 => ("欧皇", 1),
        s if s >= 56.0 => ("小欧", 2),
        s if s >= 44.0 => ("平民", 3),
        s if s >= 32.0 => ("小非酋", 4),
        s if s >= 20.0 => ("非酋", 5),
        _ => ("大非酋", 6),
    };

    Rating {
        score: (score * 10.0).round() / 10.0,
        label,
        tier,
        pity_component: (pity_component * 10.0).round() / 10.0,
        luck_component: (luck_component * 10.0).round() / 10.0,
        penalty,
        sample_sufficient,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_luck_scores_high() {
        let rating = evaluate(RatingInput {
            pulls: 500,
            avg_up_pity: Some(50.0),
            not_off_rate: Some(1.0),
            max_off_streak: 0,
        });
        assert!(rating.score >= 82.0, "score = {}", rating.score);
        assert_eq!(rating.label, "大欧皇");
    }

    #[test]
    fn terrible_luck_scores_low() {
        let rating = evaluate(RatingInput {
            pulls: 500,
            avg_up_pity: Some(150.0),
            not_off_rate: Some(0.0),
            max_off_streak: 6,
        });
        assert!(rating.score < 20.0, "score = {}", rating.score);
        assert_eq!(rating.label, "大非酋");
    }

    #[test]
    fn empty_data_falls_back_to_neutral() {
        let rating = evaluate(RatingInput {
            pulls: 0,
            avg_up_pity: None,
            not_off_rate: None,
            max_off_streak: 0,
        });
        assert_eq!(rating.score, 50.0);
        assert_eq!(rating.label, "平民");
        assert!(!rating.sample_sufficient);
    }
}
