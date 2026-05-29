use crate::world_model::LeWMModel;
use crate::metrics::id::estimate_id_local_pca;

pub struct GenomeBurst {
    queue_threshold_base: usize,
    teacher_model: LeWMModel,       // большая модель ViT-H/14
    distilled_model: Option<LeWMModel>, // маленькая модель ViT-S/16
    bongard: BongardPlus,
}

impl GenomeBurst {
    /// Проверить условие активации burst
    pub fn should_burst(&self, queue_size: usize, current_id: f64) -> bool {
        let threshold = (self.queue_threshold_base as f64 * (1.0 + current_id / 2.0)) as usize;
        queue_size >= threshold
    }

    /// Выполнить дистилляцию LeWM
    pub fn distill(&self, training_data: &[Situation]) -> Result<LeWMModel> {
        let mut student = LeWMModel::new_vit_small()?;
        for situation in training_data {
            let teacher_pred = self.teacher_model.predict(
                &situation.context_latent,
                &situation.action,
            );
            student.train_step(
                &situation.context_latent,
                &situation.action,
                &teacher_pred,  // учительская цель
            )?;
        }
        Ok(student)
    }

    /// Оценить качество сжатия
    pub fn evaluate_compression(
        &self,
        before: &LeWMModel,
        after: &LeWMModel,
        validation: &[Situation],
    ) -> f64 {
        let id_before = estimate_id_local_pca(before.encode_all(validation));
        let id_after = estimate_id_local_pca(after.encode_all(validation));
        let error_before = self.evaluate_error(before, validation);
        let error_after = self.evaluate_error(after, validation);
        (error_after - error_before) / (id_before - id_after + 1e-6)
    }
}
