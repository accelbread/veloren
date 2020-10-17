use super::{
    sentinel::{DeletedEntities, ReadTrackers, TrackedComps},
    SysTimer,
};
use crate::{
    client::{Client, RegionSubscription},
    streams::{GeneralStream, GetStream, InGameStream},
    Tick,
};
use common::{
    comp::{ForceUpdate, Inventory, InventoryUpdate, Last, Ori, Player, Pos, Vel},
    msg::ServerGeneral,
    outcome::Outcome,
    region::{Event as RegionEvent, RegionMap},
    span,
    state::TimeOfDay,
    sync::{CompSyncPackage, Uid},
    terrain::TerrainChunkSize,
    vol::RectVolSize,
};
use specs::{
    Entities, Entity as EcsEntity, Join, Read, ReadExpect, ReadStorage, System, Write, WriteStorage,
};
use vek::*;

/// This system will send physics updates to the client
pub struct Sys;
impl<'a> System<'a> for Sys {
    #[allow(clippy::type_complexity)] // TODO: Pending review in #587
    type SystemData = (
        Entities<'a>,
        Read<'a, Tick>,
        ReadExpect<'a, TimeOfDay>,
        ReadExpect<'a, RegionMap>,
        Write<'a, SysTimer<Self>>,
        ReadStorage<'a, Uid>,
        ReadStorage<'a, Pos>,
        ReadStorage<'a, Vel>,
        ReadStorage<'a, Ori>,
        ReadStorage<'a, Inventory>,
        ReadStorage<'a, RegionSubscription>,
        ReadStorage<'a, Player>,
        WriteStorage<'a, Last<Pos>>,
        WriteStorage<'a, Last<Vel>>,
        WriteStorage<'a, Last<Ori>>,
        WriteStorage<'a, Client>,
        WriteStorage<'a, InGameStream>,
        WriteStorage<'a, GeneralStream>,
        WriteStorage<'a, ForceUpdate>,
        WriteStorage<'a, InventoryUpdate>,
        Write<'a, DeletedEntities>,
        Write<'a, Vec<Outcome>>,
        TrackedComps<'a>,
        ReadTrackers<'a>,
    );

    fn run(
        &mut self,
        (
            entities,
            tick,
            time_of_day,
            region_map,
            mut timer,
            uids,
            positions,
            velocities,
            orientations,
            inventories,
            subscriptions,
            players,
            mut last_pos,
            mut last_vel,
            mut last_ori,
            mut clients,
            mut in_game_streams,
            mut general_streams,
            mut force_updates,
            mut inventory_updates,
            mut deleted_entities,
            mut outcomes,
            tracked_comps,
            trackers,
        ): Self::SystemData,
    ) {
        span!(_guard, "run", "entity_sync::Sys::run");
        timer.start();

        let tick = tick.0;
        // To send entity updates
        // 1. Iterate through regions
        // 2. Iterate through region subscribers (ie clients)
        //     - Collect a list of entity ids for clients who are subscribed to this
        //       region (hash calc to check each)
        // 3. Iterate through events from that region
        //     - For each entity entered event, iterate through the client list and
        //       check if they are subscribed to the source (hash calc per subscribed
        //       client per entity event), if not subscribed to the source send a entity
        //       creation message to that client
        //     - For each entity left event, iterate through the client list and check
        //       if they are subscribed to the destination (hash calc per subscribed
        //       client per entity event)
        // 4. Iterate through entities in that region
        // 5. Inform clients of the component changes for that entity
        //     - Throttle update rate base on distance to each client

        // Sync physics
        // via iterating through regions
        for (key, region) in region_map.iter() {
            // Assemble subscriber list for this region by iterating through clients and
            // checking if they are subscribed to this region
            let mut subscribers = (
                &mut clients,
                &entities,
                &subscriptions,
                &positions,
                &mut in_game_streams,
                &mut general_streams,
            )
                .join()
                .filter_map(
                    |(client, entity, subscription, pos, in_game_stream, general_stream)| {
                        if client.in_game.is_some() && subscription.regions.contains(&key) {
                            Some((
                                client,
                                &subscription.regions,
                                entity,
                                *pos,
                                in_game_stream,
                                general_stream,
                            ))
                        } else {
                            None
                        }
                    },
                )
                .collect::<Vec<_>>();

            for event in region.events() {
                match event {
                    RegionEvent::Entered(id, maybe_key) => {
                        // Don't process newly created entities here (redundant network messages)
                        if trackers.uid.inserted().contains(*id) {
                            continue;
                        }
                        let entity = entities.entity(*id);
                        if let Some((_uid, pos, vel, ori)) = uids.get(entity).and_then(|uid| {
                            positions.get(entity).map(|pos| {
                                (uid, pos, velocities.get(entity), orientations.get(entity))
                            })
                        }) {
                            let create_msg =
                                ServerGeneral::CreateEntity(tracked_comps.create_entity_package(
                                    entity,
                                    Some(*pos),
                                    vel.copied(),
                                    ori.copied(),
                                ));
                            for (_, regions, client_entity, _, _, general_stream) in
                                &mut subscribers
                            {
                                if maybe_key
                                    .as_ref()
                                    .map(|key| !regions.contains(key))
                                    .unwrap_or(true)
                                    // Client doesn't need to know about itself
                                    && *client_entity != entity
                                {
                                    general_stream.send_unchecked(create_msg.clone());
                                }
                            }
                        }
                    },
                    RegionEvent::Left(id, maybe_key) => {
                        // Lookup UID for entity
                        if let Some(&uid) = uids.get(entities.entity(*id)) {
                            for (_, regions, _, _, _, general_stream) in &mut subscribers {
                                if maybe_key
                                    .as_ref()
                                    .map(|key| !regions.contains(key))
                                    .unwrap_or(true)
                                {
                                    general_stream.send_unchecked(ServerGeneral::DeleteEntity(uid));
                                }
                            }
                        }
                    },
                }
            }

            // Sync tracked components
            // Get deleted entities in this region from DeletedEntities
            let (entity_sync_package, comp_sync_package) = trackers.create_sync_packages(
                &tracked_comps,
                region.entities(),
                deleted_entities
                    .take_deleted_in_region(key)
                    .unwrap_or_default(),
            );
            let entity_sync_msg = ServerGeneral::EntitySync(entity_sync_package);
            let comp_sync_msg = ServerGeneral::CompSync(comp_sync_package);
            subscribers
                .iter_mut()
                .for_each(move |(_, _, _, _, _, general_stream)| {
                    general_stream.send_unchecked(entity_sync_msg.clone());
                    general_stream.send_unchecked(comp_sync_msg.clone());
                });

            let mut send_general = |msg: ServerGeneral,
                                    entity: EcsEntity,
                                    pos: Pos,
                                    force_update: Option<&ForceUpdate>,
                                    throttle: bool| {
                for (_, _, client_entity, client_pos, _, general_stream) in &mut subscribers {
                    if if client_entity == &entity {
                        // Don't send client physics updates about itself unless force update is set
                        force_update.is_some()
                    } else if !throttle {
                        // Send the message if not throttling
                        true
                    } else {
                        // Throttle update rate based on distance to client
                        let distance_sq = client_pos.0.distance_squared(pos.0);
                        let id_staggered_tick = tick + entity.id() as u64;
                        // More entities farther away so checks start there
                        if distance_sq > 300.0f32.powi(2) {
                            id_staggered_tick % 32 == 0
                        } else if distance_sq > 250.0f32.powi(2) {
                            id_staggered_tick % 16 == 0
                        } else if distance_sq > 200.0f32.powi(2) {
                            id_staggered_tick % 8 == 0
                        } else if distance_sq > 150.0f32.powi(2) {
                            id_staggered_tick % 4 == 0
                        } else if distance_sq > 100.0f32.powi(2) {
                            id_staggered_tick % 2 == 0
                        } else {
                            true // Closer than 100 blocks
                        }
                    } {
                        general_stream.send_unchecked(msg.clone());
                    }
                }
            };

            // Sync physics components
            for (_, entity, &uid, &pos, maybe_vel, maybe_ori, force_update) in (
                region.entities(),
                &entities,
                &uids,
                &positions,
                velocities.maybe(),
                orientations.maybe(),
                force_updates.maybe(),
            )
                .join()
            {
                let mut comp_sync_package = CompSyncPackage::new();
                let mut throttle = true;
                // TODO: An entity that stopped moving on a tick that it wasn't sent to the
                // player will never have its position updated
                match last_pos.get(entity).map(|&l| l.0 != pos) {
                    Some(false) => {},
                    Some(true) => {
                        let _ = last_pos.insert(entity, Last(pos));
                        comp_sync_package.comp_modified(uid, pos);
                    },
                    None => {
                        let _ = last_pos.insert(entity, Last(pos));
                        throttle = false;
                        comp_sync_package.comp_inserted(uid, pos);
                    },
                }

                if let Some(&vel) = maybe_vel {
                    match last_vel.get(entity).map(|&l| l.0 != vel) {
                        Some(false) => {},
                        Some(true) => {
                            let _ = last_vel.insert(entity, Last(vel));
                            comp_sync_package.comp_modified(uid, vel);
                        },
                        None => {
                            let _ = last_vel.insert(entity, Last(vel));
                            throttle = false;
                            comp_sync_package.comp_inserted(uid, vel);
                        },
                    }
                } else if last_vel.remove(entity).is_some() {
                    // Send removal message if Vel was removed
                    // Note: we don't have to handle this for position because the entity will be
                    // removed from the client by the region system
                    throttle = false;
                    comp_sync_package.comp_removed::<Vel>(uid);
                }

                if let Some(&ori) = maybe_ori {
                    match last_ori.get(entity).map(|&l| l.0 != ori) {
                        Some(false) => {},
                        Some(true) => {
                            let _ = last_ori.insert(entity, Last(ori));
                            comp_sync_package.comp_modified(uid, ori);
                        },
                        None => {
                            let _ = last_ori.insert(entity, Last(ori));
                            throttle = false;
                            comp_sync_package.comp_inserted(uid, ori);
                        },
                    }
                } else if last_ori.remove(entity).is_some() {
                    // Send removal message if Ori was removed
                    throttle = false;
                    comp_sync_package.comp_removed::<Ori>(uid);
                }

                send_general(
                    ServerGeneral::CompSync(comp_sync_package),
                    entity,
                    pos,
                    force_update,
                    throttle,
                );
            }
        }

        // Handle entity deletion in regions that don't exist in RegionMap
        // (theoretically none)
        for (region_key, deleted) in deleted_entities.take_remaining_deleted() {
            for general_stream in (&mut clients, &subscriptions, &mut general_streams)
                .join()
                .filter_map(|(client, subscription, general_stream)| {
                    if client.in_game.is_some() && subscription.regions.contains(&region_key) {
                        Some(general_stream)
                    } else {
                        None
                    }
                })
            {
                for uid in &deleted {
                    general_stream.send_unchecked(ServerGeneral::DeleteEntity(Uid(*uid)));
                }
            }
        }

        // TODO: Sync clients that don't have a position?

        // Sync inventories
        for (inventory, update, in_game_stream) in
            (&inventories, &inventory_updates, &mut in_game_streams).join()
        {
            in_game_stream.send_unchecked(ServerGeneral::InventoryUpdate(
                inventory.clone(),
                update.event(),
            ));
        }

        // Sync outcomes
        for (player, pos, in_game_stream) in
            (&players, positions.maybe(), &mut in_game_streams).join()
        {
            let is_near = |o_pos: Vec3<f32>| {
                pos.zip_with(player.view_distance, |pos, vd| {
                    pos.0.xy().distance_squared(o_pos.xy())
                        < (vd as f32 * TerrainChunkSize::RECT_SIZE.x as f32).powf(2.0)
                })
            };

            let outcomes = outcomes
                .iter()
                .filter(|o| o.get_pos().and_then(&is_near).unwrap_or(true))
                .cloned()
                .collect::<Vec<_>>();
            if !outcomes.is_empty() {
                in_game_stream.send_unchecked(ServerGeneral::Outcomes(outcomes));
            }
        }
        outcomes.clear();

        // Remove all force flags.
        force_updates.clear();
        inventory_updates.clear();

        // Sync resources
        // TODO: doesn't really belong in this system (rename system or create another
        // system?)
        let tof_msg = ServerGeneral::TimeOfDay(*time_of_day);
        for general_stream in (&mut general_streams).join() {
            general_stream.send_unchecked(tof_msg.clone());
        }

        timer.end();
    }
}
