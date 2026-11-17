#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub m: [f32; 12],
}
impl Default for Matrix {
    fn default() -> Self {
        Self::identity()
    }
}
pub type Vec3 = [f32; 3];
pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn scale(a: Vec3, s: f32) -> Vec3 {
    a.map(|v| v * s)
}
pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn unit(a: Vec3) -> Option<Vec3> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 0.0001).then(|| scale(a, 1.0 / n))
}
impl Matrix {
    pub fn identity() -> Self {
        Self {
            m: [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.],
        }
    }
    pub fn col(self, c: usize) -> Vec3 {
        [self.m[c], self.m[4 + c], self.m[8 + c]]
    }
    pub fn basis(x: Vec3, y: Vec3, z: Vec3, p: Vec3) -> Self {
        Self {
            m: [
                x[0], y[0], z[0], p[0], x[1], y[1], z[1], p[1], x[2], y[2], z[2], p[2],
            ],
        }
    }
    pub fn finite(self) -> bool {
        self.m.iter().all(|v| v.is_finite())
    }
    pub fn rigid(self) -> bool {
        self.finite()
            && (0..3).all(|i| (dot(self.col(i), self.col(i)) - 1.).abs() < 0.02)
            && dot(cross(self.col(0), self.col(1)), self.col(2)) > 0.98
    }
    pub fn vector(self, v: Vec3) -> Vec3 {
        add(
            add(scale(self.col(0), v[0]), scale(self.col(1), v[1])),
            scale(self.col(2), v[2]),
        )
    }
    pub fn point(self, v: Vec3) -> Vec3 {
        add(self.col(3), self.vector(v))
    }
    pub fn mul(self, b: Self) -> Self {
        Self::basis(
            self.vector(b.col(0)),
            self.vector(b.col(1)),
            self.vector(b.col(2)),
            self.point(b.col(3)),
        )
    }
    /// Interpolate translation and shortest-path unit quaternion rotation.
    /// Matrix elements are never blended, preserving a rigid panel transform.
    pub fn smooth(self, target: Self, alpha: f32) -> Self {
        fn quaternion(m: Matrix) -> [f32; 4] {
            // Choose the largest quaternion component to remain stable at 180°.
            let candidates = [
                1. + m.m[0] + m.m[5] + m.m[10],
                1. + m.m[0] - m.m[5] - m.m[10],
                1. - m.m[0] + m.m[5] - m.m[10],
                1. - m.m[0] - m.m[5] + m.m[10],
            ];
            let i = (0..4)
                .max_by(|a, b| candidates[*a].total_cmp(&candidates[*b]))
                .unwrap();
            let c = candidates[i].max(0.).sqrt() * 0.5;
            let d = 4. * c;
            match i {
                0 => [
                    c,
                    (m.m[9] - m.m[6]) / d,
                    (m.m[2] - m.m[8]) / d,
                    (m.m[4] - m.m[1]) / d,
                ],
                1 => [
                    (m.m[9] - m.m[6]) / d,
                    c,
                    (m.m[1] + m.m[4]) / d,
                    (m.m[2] + m.m[8]) / d,
                ],
                2 => [
                    (m.m[2] - m.m[8]) / d,
                    (m.m[1] + m.m[4]) / d,
                    c,
                    (m.m[6] + m.m[9]) / d,
                ],
                _ => [
                    (m.m[4] - m.m[1]) / d,
                    (m.m[2] + m.m[8]) / d,
                    (m.m[6] + m.m[9]) / d,
                    c,
                ],
            }
        }
        let a = quaternion(self);
        let b = quaternion(target);
        let sign = if a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>() < 0. {
            -1.
        } else {
            1.
        };
        let q: [f32; 4] = std::array::from_fn(|i| a[i] + alpha * (sign * b[i] - a[i]));
        let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        let [w, x, y, z] = q.map(|v| v / length);
        Self::basis(
            [
                1. - 2. * (y * y + z * z),
                2. * (x * y + w * z),
                2. * (x * z - w * y),
            ],
            [
                2. * (x * y - w * z),
                1. - 2. * (x * x + z * z),
                2. * (y * z + w * x),
            ],
            [
                2. * (x * z + w * y),
                2. * (y * z - w * x),
                1. - 2. * (x * x + y * y),
            ],
            add(self.col(3), scale(sub(target.col(3), self.col(3)), alpha)),
        )
    }
    pub fn inverse(self) -> Self {
        let x = [self.m[0], self.m[1], self.m[2]];
        let y = [self.m[4], self.m[5], self.m[6]];
        let z = [self.m[8], self.m[9], self.m[10]];
        let r = Self::basis(x, y, z, [0.; 3]);
        Self::basis(x, y, z, scale(r.vector(self.col(3)), -1.))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub panel: usize,
    pub distance: f32,
    pub u: f32,
    pub v: f32,
}
#[derive(Clone, Copy)]
pub struct Panel {
    pub pose: Matrix,
    pub width: f32,
    pub height: f32,
    pub enabled: bool,
    /// Grip can move it. False for panels attached to a controller.
    pub grabbable: bool,
    /// SteamVR overlay curvature: 0 is flat, 1 a closed cylinder. The panel
    /// bends around its vertical axis with the edges toward the viewer.
    pub curvature: f32,
    /// Shows a placeholder: it can be grabbed, but takes no clicks or scrolling.
    pub passive: bool,
}
impl Panel {
    /// World position of panel coordinates (`u` right, `v` down, 0..1),
    /// following the same curve as `nearest`.
    pub fn point(&self, u: f32, v: f32) -> Vec3 {
        let x = (u - 0.5) * self.width;
        let y = (0.5 - v) * self.height;
        if self.curvature <= 0.001 {
            return self.pose.point([x, y, 0.]);
        }
        let radius = self.width / (std::f32::consts::TAU * self.curvature);
        let angle = x / radius;
        self.pose
            .point([radius * angle.sin(), y, radius * (1. - angle.cos())])
    }
}
/// Ray against a curved panel: an arc of a vertical cylinder whose radius is
/// width / (2π · curvature) and whose axis lies that far in front of the
/// panel centre. Returns distance, U and the local height of the hit.
fn curved_hit(p: &Panel, origin: Vec3, direction: Vec3) -> Option<(f32, f32, f32)> {
    let radius = p.width / (std::f32::consts::TAU * p.curvature);
    let to_local = p.pose.inverse();
    let o = to_local.point(origin);
    let d = to_local.vector(direction);
    // Circle in the local XZ plane, centred at (0, radius).
    let (ox, oz) = (o[0], o[2] - radius);
    let a = d[0] * d[0] + d[2] * d[2];
    let b = 2. * (ox * d[0] + oz * d[2]);
    let c = ox * ox + oz * oz - radius * radius;
    let root = (b * b - 4. * a * c).sqrt();
    if a < 0.000001 || !root.is_finite() {
        return None;
    }
    [(-b - root) / (2. * a), (-b + root) / (2. * a)]
        .into_iter()
        .filter(|t| *t > 0.)
        .filter_map(|t| {
            let (x, z) = (ox + t * d[0], oz + t * d[2]);
            // Only the concave side is the front of the panel.
            if x * d[0] + z * d[2] <= 0. {
                return None;
            }
            let arc = x.atan2(-z) * radius;
            Some((t, arc / p.width + 0.5, o[1] + t * d[1]))
        })
        .find(|(_, u, _)| (0. ..=1.).contains(u))
}
pub fn nearest(ray: Matrix, panels: &[Panel]) -> Option<Hit> {
    if !ray.rigid() {
        return None;
    }
    let origin = ray.col(3);
    let direction = scale(ray.col(2), -1.);
    panels
        .iter()
        .enumerate()
        .filter_map(|(panel, p)| {
            if !p.enabled
                || !p.pose.rigid()
                || !p.width.is_finite()
                || !p.height.is_finite()
                || p.width <= 0.
                || p.height <= 0.
            {
                return None;
            }
            let (distance, u, y) = if p.curvature > 0.001 {
                curved_hit(p, origin, direction)?
            } else {
                let normal = p.pose.col(2);
                let denom = dot(direction, normal);
                if denom >= -0.0001 {
                    return None;
                }
                let distance = dot(sub(p.pose.col(3), origin), normal) / denom;
                let local = p
                    .pose
                    .inverse()
                    .point(add(origin, scale(direction, distance)));
                (distance, local[0] / p.width + 0.5, local[1])
            };
            let v = 0.5 - y / p.height;
            if !(0.005..=20.).contains(&distance)
                || !(0. ..=1.).contains(&u)
                || !(0. ..=1.).contains(&v)
            {
                return None;
            }
            Some(Hit {
                panel,
                distance,
                u,
                v,
            })
        })
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}
/// How squarely a panel's front faces `eye`: 1 seen head-on, 0 edge-on,
/// negative from behind.
pub fn frontness(pose: Matrix, eye: Vec3) -> f32 {
    unit(sub(eye, pose.col(3))).map_or(1., |to_eye| dot(pose.col(2), to_eye))
}
/// Upright pose facing the head: `distance` metres ahead along the head's
/// level forward direction, shifted `right` and `up` metres.
pub fn placement(head: Matrix, distance: f32, right: f32, up: f32) -> Option<Matrix> {
    if !head.rigid() {
        return None;
    }
    let f = unit([-head.m[2], 0., -head.m[10]])?;
    let x = [-f[2], 0., f[0]];
    Some(Matrix::basis(
        x,
        [0., 1., 0.],
        scale(f, -1.),
        add(
            head.col(3),
            add(add(scale(f, distance), scale(x, right)), [0., up, 0.]),
        ),
    ))
}
/// Like `placement`, but tilted so the panel squarely faces the head, for
/// panels placed well above or below eye level.
pub fn facing(head: Matrix, distance: f32, right: f32, up: f32) -> Option<Matrix> {
    let centre = placement(head, distance, right, up)?.col(3);
    Some(aim(centre, sub(scale(centre, 2.), head.col(3))))
}
/// Pointer pose at `origin` whose -Z axis passes through `target`.
pub fn aim(origin: Vec3, target: Vec3) -> Matrix {
    let z = unit(sub(origin, target)).unwrap_or([0., 0., 1.]);
    let x = unit(cross([0., 1., 0.], z)).unwrap_or([1., 0., 0.]);
    Matrix::basis(x, cross(z, x), z, origin)
}
// Beam plane faces the observer as closely as the ray allows. Endpoint is at
// local +Y; native draws a thin rectangle and a separately oriented dot.
pub fn beam(ray: Matrix, length: f32, eye: Vec3) -> Matrix {
    let y = scale(ray.col(2), -1.);
    let p = add(ray.col(3), scale(y, length * 0.5));
    let x = unit(cross(y, sub(eye, p))).unwrap_or(ray.col(0));
    let z = cross(x, y);
    Matrix::basis(x, y, z, p)
}
pub fn endpoint(ray: Matrix, length: f32, eye: Vec3) -> Matrix {
    let p = ray.point([0., 0., -length]);
    let z = unit(sub(eye, p)).unwrap_or(ray.col(2));
    let x = unit(cross([0., 1., 0.], z)).unwrap_or_else(|| unit(cross([1., 0., 0.], z)).unwrap());
    let y = cross(z, x);
    Matrix::basis(x, y, z, add(p, scale(z, 0.002)))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn panel(z: f32) -> Panel {
        let mut pose = Matrix::identity();
        pose.m[11] = z;
        Panel {
            pose,
            width: 2.,
            height: 1.,
            enabled: true,
            grabbable: true,
            curvature: 0.,
            passive: false,
        }
    }
    #[test]
    fn nearest_front_edges_and_invalid() {
        let panels = [panel(-2.), panel(-1.)];
        let h = nearest(Matrix::identity(), &panels).unwrap();
        assert_eq!(h.panel, 1);
        assert_eq!((h.u, h.v), (0.5, 0.5));
        let mut r = Matrix::identity();
        r.m[3] = 1.;
        assert_eq!(nearest(r, &panels).unwrap().u, 1.);
        r.m[3] = 1.01;
        assert!(nearest(r, &panels).is_none());
        r.m[3] = f32::NAN;
        assert!(nearest(r, &panels).is_none());
        let mut back = panel(1.);
        back.pose.m[0] = -1.;
        back.pose.m[10] = -1.;
        assert!(nearest(Matrix::identity(), &[back]).is_none());
    }
    #[test]
    fn aiming_at_a_panel_point_hits_the_same_coordinates() {
        let origin = [0.3, -0.2, 0.];
        for curvature in [0., 0.12, 0.3] {
            let mut p = panel(-2.);
            p.curvature = curvature;
            for (u, v) in [(0.5, 0.5), (0.9, 0.04), (0.05, 0.95)] {
                let target = p.point(u, v);
                let z = unit(sub(origin, target)).unwrap();
                let x = unit(cross([0., 1., 0.], z)).unwrap();
                let ray = Matrix::basis(x, cross(z, x), z, origin);
                let hit = nearest(ray, &[p]).unwrap();
                assert!(
                    (hit.u - u).abs() < 0.0005 && (hit.v - v).abs() < 0.0005,
                    "curvature {curvature}: aimed at ({u}, {v}), hit ({}, {})",
                    hit.u,
                    hit.v
                );
            }
        }
    }
    #[test]
    fn curved_panel_matches_flat_at_centre_and_bends_toward_viewer() {
        let mut curved = panel(-2.);
        curved.curvature = 0.2;
        let centre = nearest(Matrix::identity(), &[curved]).unwrap();
        assert!((centre.distance - 2.).abs() < 0.0001 && (centre.u - 0.5).abs() < 0.0001);
        let mut ray = Matrix::identity();
        ray.m[3] = 0.8;
        let flat = nearest(ray, &[panel(-2.)]).unwrap();
        let bent = nearest(ray, &[curved]).unwrap();
        assert!(bent.distance < flat.distance - 0.05);
        // The arc is longer than its chord, so the same X lies further along U.
        assert!(bent.u > flat.u && bent.u < 1.);
        ray.m[3] = 0.;
        ray.m[11] = -3.;
        assert!(nearest(ray, &[curved]).is_none(), "behind the panel");
    }
    #[test]
    fn frontness_runs_from_head_on_to_behind() {
        let p = panel(-2.).pose;
        assert!((frontness(p, [0.; 3]) - 1.).abs() < 0.0001);
        assert!(frontness(p, [0., 0., -4.]) < -0.99);
        // 45 degrees to the side.
        assert!((frontness(p, [2., 0., 0.]) - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.001);
    }
    #[test]
    fn grab_relative_transform_keeps_rotation_and_release() {
        let mut hand = Matrix::basis([0., 0., -1.], [0., 1., 0.], [1., 0., 0.], [1., 2., 3.]);
        let p = panel(-2.).pose;
        let relative = hand.inverse().mul(p);
        assert_matrix(hand.mul(relative), p);
        hand.m[3] += 2.;
        let moved = hand.mul(relative);
        let mut expected = p;
        expected.m[3] += 2.;
        assert_matrix(moved, expected);
    }
    fn assert_matrix(a: Matrix, b: Matrix) {
        for (x, y) in a.m.iter().zip(b.m) {
            assert!((x - y).abs() < 0.0001);
        }
    }
    #[test]
    fn initial_anchor_faces_head() {
        let p = placement(Matrix::identity(), 1.6, 0., 0.15).unwrap();
        assert_eq!(p.m[11], -1.6);
        assert!(
            nearest(
                Matrix::identity(),
                &[Panel {
                    pose: p,
                    width: 2.,
                    height: 2.,
                    enabled: true,
                    grabbable: true,
                    curvature: 0.,
                    passive: false,
                }]
            )
            .is_some()
        );
    }
    #[test]
    fn a_panel_placed_below_eye_level_tilts_up_to_face_the_head() {
        let p = facing(Matrix::identity(), 1.3, 0., -0.6).unwrap();
        assert!(p.rigid());
        assert_eq!((p.m[7], p.m[11]), (-0.6, -1.3));
        // Its normal points from its centre at the head.
        let to_head = unit(sub([0.; 3], p.col(3))).unwrap();
        assert!(dot(p.col(2), to_head) > 0.9999);
        assert!(p.col(0)[1].abs() < 0.0001, "no roll");
    }
    #[test]
    fn beam_length_and_billboard_are_finite_even_when_eye_collinear() {
        let ray = Matrix::identity();
        for eye in [[0., 0., 0.], [0., 0., -2.], [0.5, 1., 0.]] {
            let b = beam(ray, 2., eye);
            assert!(b.rigid());
            assert_matrix(
                Matrix::basis(
                    b.col(0),
                    b.col(1),
                    b.col(2),
                    add(b.col(3), scale(b.col(1), 1.)),
                ),
                Matrix::basis(b.col(0), b.col(1), b.col(2), [0., 0., -2.]),
            );
            let marker = endpoint(ray, 2., eye);
            assert!(marker.rigid());
            assert!(dot(marker.col(2), sub(eye, marker.col(3))) >= -0.003);
        }
    }
}
