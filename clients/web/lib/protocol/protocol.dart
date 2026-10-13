// The wire protocol as the client uses it.
//
// The types themselves, their JSON, their defaults, the labels and the
// constants Rust holds are in protocol.g.dart, which is generated from
// shared/src (see its header) and is never edited. This file holds what the
// client adds to them. Nothing here names a wire key or a tag.

import 'dart:typed_data';

import 'hex.dart';
import 'protocol.g.dart';

export 'hex.dart';
export 'protocol.g.dart';

/// Whether [name] is a built-in bot's.
bool isBotName(String name) => name.toUpperCase().startsWith(botNamePrefix.toUpperCase());

/// The band of a ratio, as the server's `scaling::ratio_label` cuts it. The
/// server holds a ratio as an `f32`, so [ratio] is rounded to one first and
/// the edges fall where the server's do. The ratio itself only the server
/// can work out; the demo world uses this to label the ones it makes up.
RatioLabel ratioLabelFor(double ratio) {
  final r = _f32(ratio);
  if (r >= ratioSafe) return RatioLabel.safe;
  if (r >= ratioFavourable) return RatioLabel.favourable;
  if (r >= ratioEven) return RatioLabel.even;
  if (r >= ratioRisky) return RatioLabel.risky;
  return RatioLabel.deadly;
}

double _f32(double x) => (Float32List(1)..[0] = x)[0];

/// Whether a stockpile holding [stock] of [cap] is at the fill where the
/// server's `StorageNearCap` fires. The server divides one amount by the
/// other, both `f64` as they are here, and compares the quotient with
/// [storageNearCapRatio]; this is the same division, so the edge falls where
/// the server's does.
bool storageNearCap(num stock, num cap) => cap > 0 && stock.toDouble() / cap.toDouble() >= storageNearCapRatio;

/// What a `refine` order loses against a dock, as a whole percent.
const refineLossPct = (refineLoss * 100 + 0.5) ~/ 1;

/// The share of the ships at home, and of the Solar Satellites, that a raid
/// which breaks through destroys, as a whole percent.
const raidLostShipPct = (raidLostShipFraction * 100 + 0.5) ~/ 1;

/// The zones of the world. A ring's zone is cut as the engine's
/// `Zone::from_distance` cuts it, at the radii Rust holds; the names are the
/// client's.
enum WorldZone {
  centralHub('Central Hub'),
  innerRing('Inner Ring'),
  outerRing('Outer Ring'),
  wandering('The Wandering');

  const WorldZone(this.label);
  final String label;

  static WorldZone ofRing(int ring) => ring <= 0
      ? centralHub
      : ring <= zoneInnerRingRadius
          ? innerRing
          : ring <= zoneOuterRingRadius
              ? outerRing
              : wandering;
}

extension HexZone on Hex {
  WorldZone get zone => WorldZone.ofRing(distFromOrigin);
}

extension BatchWaitReason on BatchWait {
  WaitReason get reason => switch (this) {
        BatchWait.slot => WaitReason.slot,
        BatchWait.prerequisite => WaitReason.prerequisite,
      };
}

extension ShipyardItemLabel on ShipyardItem {
  String get label => ship?.label ?? defence!.label;
}

extension ResourceKindOf on ResourceKind {
  double of(Resources r) => switch (this) {
        ResourceKind.metal => r.metal,
        ResourceKind.crystal => r.crystal,
        ResourceKind.deuterium => r.deuterium,
      };
}

extension ResourcesTotal on Resources {
  double get total => metal + crystal + deuterium;
}

extension ResourceEtaOf on ResourceEta {
  int? of(ResourceKind k) => switch (k) {
        ResourceKind.metal => metal,
        ResourceKind.crystal => crystal,
        ResourceKind.deuterium => deuterium,
      };
}

extension WorldInfoHub on WorldInfo {
  /// What the hub offers at [at]; nothing anywhere else.
  List<HubService> servicesAt(Hex at) {
    for (final site in hub) {
      if (site.location == at) return site.services;
    }
    return const [];
  }

  /// The hub sector nearest [from] that offers [service].
  Hex? nearest(HubService service, Hex from) {
    Hex? best;
    for (final site in hub) {
      if (!site.services.contains(service)) continue;
      if (best == null || Hex.distance(site.location, from) < Hex.distance(best, from)) best = site.location;
    }
    return best;
  }

  /// "x600 blitz" or "x42".
  String get label {
    final n = pace == pace.roundToDouble() ? pace.round().toString() : pace.toString();
    return preset == null ? 'x$n' : 'x$n $preset';
  }
}

extension ExchangeMeterFill on ExchangeMeter {
  /// How full the meter is, 0 to 1; full when it has no span.
  double get fill => span > 0 ? (pressure / span).clamp(0, 1).toDouble() : 1;
}

extension MovePreviewOdds on MovePreview {
  /// The odds part of the preview.
  SectorOdds get odds => SectorOdds(sector: target, threat: threat, ratio: ratio, label: label);
}

extension NpcFleetInfoCount on NpcFleetInfo {
  int get shipCount => ships.fold(0, (s, e) => s + e.count);
}

extension RequirementLabel on Requirement {
  /// As cards and captions print it, in the engine's words: "Shipyard level
  /// 4", or "300000 energy made" for one that asks for energy.
  String get label => energy ? '$need energy made' : '$name level $need';
}

extension RapidFireOf on WorldInfo {
  /// What [c] fires again at, in the catalogue's order.
  List<RapidFire> rapidFireOf(ShipClass c) => [for (final r in rapidFire) if (r.shipClass == c) r];

  /// The classes that fire again at [target].
  List<RapidFire> rapidFireAgainst(ShipyardItem target) => [for (final r in rapidFire) if (r.against == target) r];
}

extension CatalogStorage on HomeworldCatalog {
  /// The building that stores [kind], which Rust names by its label.
  BuildingOption? storageOf(ResourceKind kind) {
    for (final b in buildings) {
      if (b.buildingType.label == kind.storage) return b;
    }
    return null;
  }
}

extension HomeworldStateEdit on HomeworldState {
  int buildingLevel(BuildingType t) {
    for (final b in buildings) {
      if (b.buildingType == t) return b.level;
    }
    return 0;
  }

  /// A copy with some fields replaced (for the scripted world of the tests).
  HomeworldState copyWith({
    Resources? production,
    EnergyState? energy,
    int? satellites,
    StorageState? storage,
    List<BuildingState>? buildings,
    List<ResearchState>? research,
    List<ShipState>? dockedShips,
    List<DefenceState>? defences,
    double? homeDefencePower,
    double? dockedPower,
    double? satellitePower,
    double? structurePower,
    List<BuildQueueItem>? buildQueue,
    List<QueuedBuild>? buildPending,
    int? buildSlots,
    Requirement? buildSlotUnlock,
    bool clearBuildSlotUnlock = false,
    ShipyardQueueItem? shipyardQueue,
    bool clearShipyardQueue = false,
    List<QueuedShip>? shipyardPending,
    ResearchItem? researchActive,
    bool clearResearchActive = false,
    List<QueuedResearch>? researchPending,
    HomeworldCatalog? catalog,
  }) =>
      HomeworldState(
        location: location,
        production: production ?? this.production,
        energy: energy ?? this.energy,
        satellites: satellites ?? this.satellites,
        storage: storage ?? this.storage,
        buildings: buildings ?? this.buildings,
        research: research ?? this.research,
        buildQueue: buildQueue ?? this.buildQueue,
        buildPending: buildPending ?? this.buildPending,
        buildSlots: buildSlots ?? this.buildSlots,
        buildSlotUnlock: clearBuildSlotUnlock ? null : (buildSlotUnlock ?? this.buildSlotUnlock),
        queueWaitingMax: queueWaitingMax,
        shipyardQueue: clearShipyardQueue ? null : (shipyardQueue ?? this.shipyardQueue),
        shipyardPending: shipyardPending ?? this.shipyardPending,
        researchActive: clearResearchActive ? null : (researchActive ?? this.researchActive),
        researchPending: researchPending ?? this.researchPending,
        dockedShips: dockedShips ?? this.dockedShips,
        defences: defences ?? this.defences,
        homeDefencePower: homeDefencePower ?? this.homeDefencePower,
        dockedPower: dockedPower ?? this.dockedPower,
        satellitePower: satellitePower ?? this.satellitePower,
        structurePower: structurePower ?? this.structurePower,
        nextRaidEstimatePower: nextRaidEstimatePower,
        catalog: catalog ?? this.catalog,
      );
}
