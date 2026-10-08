enum Terrain {
  empty('Empty'),
  asteroid('Asteroid Field'),
  nebula('Nebula'),
  debris('Debris Field'),
  anomaly('Anomaly');

  final String label;
  const Terrain(this.label);
}

enum Density {
  none('None'),
  sparse('Sparse'),
  moderate('Moderate'),
  rich('Rich'),
  pristine('Pristine');

  final String label;
  const Density(this.label);
}

class SectorState {
  final int q;
  final int r;
  final Terrain terrain;
  final Density resMetal;
  final Density resCrystal;
  final Density resDeut;
  final bool hasHostile;
  final int hostileCount;
  final double prunePct;
  final bool explored;
  final int dist;

  const SectorState({
    required this.q,
    required this.r,
    required this.terrain,
    required this.resMetal,
    required this.resCrystal,
    required this.resDeut,
    required this.hasHostile,
    required this.hostileCount,
    required this.prunePct,
    required this.explored,
    required this.dist,
  });
}
